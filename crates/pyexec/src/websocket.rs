use crate::error::ServiceError;
use crate::python_vm;
use crate::rpc::{handle_rpc_request, create_client_message, RpcMessage, RpcResponse};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::{
    accept_async,
    tungstenite::Message,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub async fn accept_connections(listener: TcpListener) -> Result<(), ServiceError> {
    info!("Waiting for WebSocket connections...");

    while let Ok((stream, addr)) = listener.accept().await {
        info!("Accepted connection from: {}", addr);
        tokio::spawn(handle_connection(stream, addr));
    }

    Ok(())
}

async fn handle_connection(stream: TcpStream, addr: SocketAddr) {
    let session_id = Uuid::new_v4();
    info!("New session established: {} for client {}", session_id, addr);

    // Create a channel for Python code to send messages back to client
    let (msg_sender, mut msg_receiver) = mpsc::unbounded_channel::<String>();

    // Initialize a Python interpreter for this session and register the message sender
    let interpreter = python_vm::register_session(session_id);
    python_vm::register_message_sender(session_id, msg_sender);

    match accept_async(stream).await {
        Ok(ws_stream) => {
            info!("WebSocket connection established: {}", addr);

            let (ws_sender, mut ws_receiver) = ws_stream.split();

            // Handle messages from Python to client
            let ws_sender_arc = Arc::new(Mutex::new(ws_sender));
            let ws_sender_clone = ws_sender_arc.clone();
            let session_id_clone = session_id.clone();
            
            // Spawn a task to handle messages from Python to client
            let python_to_client = tokio::spawn(async move {
                while let Some(message) = msg_receiver.recv().await {
                    match create_client_message(&session_id_clone, &message) {
                        Ok(json) => {
                            if let Err(e) = ws_sender_clone.lock().await.send(Message::Text(json)).await {
                                error!("Error sending Python message to client: {:?}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error creating client message: {:?}", e);
                        }
                    }
                }
            });

            let session_id_clone = session_id.clone();
            // Handle incoming WebSocket messages from client
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("Received message: {}", text);

                        // Parse the JSON RPC message
                        match serde_json::from_str::<RpcMessage>(&text) {
                            Ok(rpc_message) => {
                                // let session_id_copy = session_id_clone.clone();
                                
                                // Process the RPC request asynchronously
                                let response_future = handle_rpc_request(session_id_clone, rpc_message);
                                
                                // Spawn a task to handle the RPC request
                                let ws_sender_clone = ws_sender_arc.clone();
                                tokio::spawn(async move {
                                    let response = match response_future.await {
                                        Ok(response) => response,
                                        Err(err) => {
                                            error!("Error handling RPC request: {:?}", err);
                                            RpcResponse::error(
                                                Uuid::nil(), // Use nil UUID for internal errors
                                                500,
                                                format!("Internal server error: {}", err),
                                            )
                                        }
                                    };

                                    // Serialize and send response
                                    match serde_json::to_string(&response) {
                                        Ok(json) => {
                                            debug!("Sending response: {}", json);
                                            if let Err(e) = ws_sender_clone.lock().await.send(Message::Text(json)).await {
                                                error!("Error sending response: {:?}", e);
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error serializing response: {:?}", e);
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                error!("Error parsing RPC request: {:?}", e);
                                
                                // Try to parse just the ID for a proper error response
                                let id = match serde_json::from_str::<Value>(&text) {
                                    Ok(Value::Object(map)) => {
                                        map.get("id").and_then(|id| id.as_str().map(|s| s.to_string()))
                                            .unwrap_or_else(|| "unknown".to_string())
                                    }
                                    _ => "unknown".to_string(),
                                };
                                
                                let error_response = RpcResponse::error(
                                    Uuid::nil(),
                                    400,
                                    format!("Invalid RPC request: {}", e),
                                );
                                
                                if let Ok(json) = serde_json::to_string(&error_response) {
                                    if let Err(e) = ws_sender_arc.lock().await.send(Message::Text(json)).await {
                                        error!("Error sending error response: {:?}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(reason)) => {
                        info!(
                            "WebSocket closed by client: {}. Reason: {:?}",
                            addr, reason
                        );
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        // Automatically respond to ping messages
                        if let Err(e) = ws_sender_arc.lock().await.send(Message::Pong(data)).await {
                            error!("Error sending pong: {:?}", e);
                            break;
                        }
                    }
                    Ok(_) => {
                        // Ignore other message types
                    }
                    Err(e) => {
                        error!("Error receiving message: {:?}", e);
                        break;
                    }
                }
            }

            // Abort the Python-to-client task when the connection is closed
            python_to_client.abort();
        }
        Err(e) => {
            error!("Error during WebSocket handshake: {}", e);
        }
    }

    // Clean up resources for this session
    python_vm::unregister_session(&session_id);
    info!("Session terminated: {} for client {}", session_id, addr);
}