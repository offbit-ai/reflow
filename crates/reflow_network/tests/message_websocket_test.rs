use reflow_network::websocket_rpc::{WebSocketRpcClient, types::*};
use reflow_actor::message::Message;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use futures_util::{SinkExt, StreamExt};

/// Simple test WebSocket server that echoes messages
async fn start_test_websocket_server(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await.unwrap();
        
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let ws_stream = accept_async(stream).await.unwrap();
                let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                
                while let Some(msg) = ws_receiver.next().await {
                    if let Ok(WsMessage::Text(text)) = msg {
                        // Parse as JSON-RPC
                        if let Ok(request) = serde_json::from_str::<RpcRequest>(&text) {
                            // Echo back with test response
                            let response = RpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id,
                                result: Some(json!({
                                    "echo": request.params,
                                    "method": request.method,
                                    "test": true
                                })),
                                error: None,
                            };
                            
                            let response_text = serde_json::to_string(&response).unwrap();
                            ws_sender.send(WsMessage::Text(response_text.into())).await.unwrap();
                        }
                    }
                }
            });
        }
    })
}

/// Test Message serialization over WebSocket RPC
#[tokio::test]
async fn test_message_websocket_serialization() {
    // Start test server
    let server_handle = start_test_websocket_server(5580).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client
    let client = Arc::new(WebSocketRpcClient::new(
        "ws://127.0.0.1:5580".to_string()
    ));
    
    client.connect().await.unwrap();
    
    // Test various Message types
    let test_messages = vec![
        ("integer", Message::Integer(42)),
        ("float", Message::Float(3.14)),
        ("string", Message::string("hello world".to_string())),
        ("boolean", Message::Boolean(true)),
        ("array", Message::array(vec![
            Value::from(1).into(),
            Value::from("test").into(),
        ])),
        ("object", Message::object(json!({"key": "value"}).into())),
    ];
    
    for (name, msg) in test_messages {
        // Convert to JSON
        let json_value: Value = msg.into();
        
        // Send via RPC
        let response = client.call("test", json!({
            "type": name,
            "data": json_value
        })).await.unwrap();
        
        // Verify echo
        assert_eq!(response["method"], "test");
        assert_eq!(response["test"], true);
        assert!(response["echo"]["type"].is_string());
    }
    
    client.disconnect().await.unwrap();
    server_handle.abort();
}

/// Test large message compression
#[tokio::test]
async fn test_large_message_websocket() {
    // Start test server
    let server_handle = start_test_websocket_server(5581).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client
    let client = Arc::new(WebSocketRpcClient::new(
        "ws://127.0.0.1:5581".to_string()
    ));
    
    client.connect().await.unwrap();
    
    // Create large message (>1KB)
    let large_string = "x".repeat(2000); // 2KB
    let large_msg = Message::string(large_string.clone());
    let large_value: Value = large_msg.into();
    
    // Send large message
    let response = client.call("large_test", json!({
        "data": large_value
    })).await.unwrap();
    
    // Verify it was transmitted
    assert_eq!(response["method"], "large_test");
    assert!(response["echo"]["data"].as_str().unwrap().len() == 2000);
    
    // Test with binary data
    let binary_data = vec![0u8; 1500]; // 1.5KB
    let stream_msg = Message::Stream(Arc::new(binary_data.clone()));
    let stream_value: Value = stream_msg.into();
    
    let response = client.call("binary_test", json!({
        "binary": stream_value
    })).await.unwrap();
    
    assert_eq!(response["method"], "binary_test");
    // Binary becomes array of numbers in JSON
    assert!(response["echo"]["binary"].is_array());
    
    client.disconnect().await.unwrap();
    server_handle.abort();
}

/// Test bidirectional communication with outputs
#[tokio::test]
async fn test_websocket_bidirectional() {
    // Start a server that sends notifications
    let server_handle = tokio::spawn(async move {
        let addr = "127.0.0.1:5582";
        let listener = TcpListener::bind(&addr).await.unwrap();
        
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let ws_stream = accept_async(stream).await.unwrap();
                let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                
                while let Some(msg) = ws_receiver.next().await {
                    if let Ok(WsMessage::Text(text)) = msg {
                        if let Ok(request) = serde_json::from_str::<RpcRequest>(&text) {
                            // Send response
                            let response = RpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: request.id.clone(),
                                result: Some(json!({"started": true})),
                                error: None,
                            };
                            ws_sender.send(WsMessage::Text(
                                serde_json::to_string(&response).unwrap().into()
                            )).await.unwrap();
                            
                            // Send some notifications
                            for i in 0..3 {
                                let notification = json!({
                                    "jsonrpc": "2.0",
                                    "method": "script_output",
                                    "params": {
                                        "actor_id": "test_actor",
                                        "port": "output",
                                        "data": format!("chunk_{}", i),
                                        "timestamp": chrono::Utc::now().timestamp_millis() as u64
                                    }
                                });
                                
                                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                ws_sender.send(WsMessage::Text(
                                    notification.to_string().into()
                                )).await.unwrap();
                            }
                        }
                    }
                }
            });
        }
    });
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Create client with output channel
    let client = Arc::new(WebSocketRpcClient::new(
        "ws://127.0.0.1:5582".to_string()
    ));
    
    let (output_tx, output_rx) = flume::unbounded();
    client.set_output_channel(output_tx);
    client.connect().await.unwrap();
    
    // Start process
    let response = client.call("start", json!({})).await.unwrap();
    assert_eq!(response["started"], true);
    
    // Collect outputs
    let mut outputs = Vec::new();
    let timeout = tokio::time::Duration::from_secs(2);
    let start = std::time::Instant::now();
    
    while outputs.len() < 3 && start.elapsed() < timeout {
        if let Ok(output) = output_rx.try_recv() {
            outputs.push(output);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
    
    // Verify we received notifications
    assert_eq!(outputs.len(), 3);
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output.actor_id, "test_actor");
        assert_eq!(output.port, "output");
        assert_eq!(output.data, format!("chunk_{}", i));
    }
    
    client.disconnect().await.unwrap();
    server_handle.abort();
}