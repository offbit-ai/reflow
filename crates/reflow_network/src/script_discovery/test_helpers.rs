#[cfg(test)]
pub mod test_server {
    use crate::websocket_rpc::{RpcRequest, RpcResponse, RpcNotification};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};
    use futures_util::{SinkExt, StreamExt};
    use tokio::time::{Duration, timeout, sleep};
    use serde_json::json;
    
    pub struct TestWebSocketServer {
        pub port: u16,
        pub url: String,
        shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
        server_handle: Option<tokio::task::JoinHandle<()>>,
    }
    
    impl TestWebSocketServer {
        /// Start a test WebSocket server that simulates a script actor runtime
        pub async fn start() -> Self {
            // Bind to a random available port
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let port = addr.port();
            let url = format!("ws://127.0.0.1:{}", port);
            
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
            
            // Start the server in a background task - don't await it!
            let server_handle = tokio::spawn(async move {
                loop {
                    // Use try_recv to check for shutdown without blocking
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }
                    
                    // Accept with a short timeout so we can check for shutdown periodically
                    match timeout(Duration::from_millis(100), listener.accept()).await {
                        Ok(Ok((stream, _))) => {
                            // Handle each connection in its own task
                            tokio::spawn(async move {
                                if let Ok(ws_stream) = accept_async(stream).await {
                                    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                                    
                                    while let Some(msg_result) = ws_receiver.next().await {
                                        match msg_result {
                                            Ok(Message::Text(text)) => {
                                                println!("Test server received message: {}", text);
                                                if let Ok(request) = serde_json::from_str::<RpcRequest>(&text) {
                                                    println!("Parsed request with method: {}", request.method);
                                                    TestWebSocketServer::handle_request(&mut ws_sender, request).await;
                                                } else {
                                                    println!("Failed to parse request");
                                                }
                                            }
                                            Ok(Message::Close(_)) => break,
                                            _ => {}
                                        }
                                    }
                                }
                            });
                        }
                        Ok(Err(_)) => {
                            // Accept failed, continue
                        }
                        Err(_) => {
                            // Timeout, check for shutdown and continue
                        }
                    }
                }
            });
            
            // Small delay to ensure the server is listening
            tokio::time::sleep(Duration::from_millis(10)).await;
            
            TestWebSocketServer {
                port,
                url,
                shutdown_tx: Some(shutdown_tx),
                server_handle: Some(server_handle),
            }
        }
        
        async fn handle_request(
            ws_sender: &mut futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
                Message
            >,
            request: RpcRequest,
        ) {
            println!("Handling request with method: {} and id: {}", request.method, request.id);
            match request.method.as_str() {
                "process" => {
                    println!("Processing 'process' request");
                    // Send response
                    let response = RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id.clone(),
                        result: Some(json!({
                            "outputs": {
                                "output": {
                                    "type": "string",
                                    "value": "processed"
                                }
                            }
                        })),
                        error: None,
                    };
                    
                    let response_text = serde_json::to_string(&response).unwrap();
                    println!("Sending response: {}", response_text);
                    let send_result = ws_sender.send(Message::text(response_text)).await;
                    println!("Response send result: {:?}", send_result);
                    
                    // Send async output notification after a delay
                    sleep(Duration::from_millis(10)).await;
                    
                    let notification = RpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "output".to_string(),
                        params: json!({
                            "actor_id": "test_actor",
                            "port": "async_output",
                            "data": {
                                "type": "string",
                                "value": "async data"
                            },
                            "timestamp": 123456789
                        }),
                    };
                    
                    let notification_text = serde_json::to_string(&notification).unwrap();
                    let _ = ws_sender.send(Message::text(notification_text)).await;
                }
                "stream" => {
                    // Send initial response
                    let response = RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id.clone(),
                        result: Some(json!({
                            "status": "streaming"
                        })),
                        error: None,
                    };
                    
                    let response_text = serde_json::to_string(&response).unwrap();
                    println!("Sending response: {}", response_text);
                    let send_result = ws_sender.send(Message::text(response_text)).await;
                    println!("Response send result: {:?}", send_result);
                    
                    // Send multiple streaming outputs
                    for i in 0..3 {
                        sleep(Duration::from_millis(10)).await;
                        
                        let notification = RpcNotification {
                            jsonrpc: "2.0".to_string(),
                            method: "output".to_string(),
                            params: json!({
                                "actor_id": "streaming_actor",
                                "port": "stream",
                                "data": {
                                    "type": "integer",
                                    "value": i
                                },
                                "timestamp": chrono::Utc::now().timestamp_millis() as u64
                            }),
                        };
                        
                        let notification_text = serde_json::to_string(&notification).unwrap();
                        let _ = ws_sender.send(Message::text(notification_text)).await;
                    }
                }
                _ => {
                    // Unknown method
                    let response = RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(crate::websocket_rpc::RpcError {
                            code: -32601,
                            message: "Method not found".to_string(),
                            data: None,
                        }),
                    };
                    
                    let response_text = serde_json::to_string(&response).unwrap();
                    println!("Sending response: {}", response_text);
                    let send_result = ws_sender.send(Message::text(response_text)).await;
                    println!("Response send result: {:?}", send_result);
                }
            }
        }
        
        /// Shutdown the server gracefully
        pub async fn shutdown(mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
            
            if let Some(handle) = self.server_handle.take() {
                // Wait for server to shutdown with timeout
                let _ = timeout(Duration::from_secs(1), handle).await;
            }
        }
    }
    
    impl Drop for TestWebSocketServer {
        fn drop(&mut self) {
            // Send shutdown signal if not already sent
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
        }
    }
}