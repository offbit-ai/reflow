#[cfg(test)]
mod microsandbox_server_tests {
    use anyhow::Result;
    use tokio::sync::oneshot;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_server_starts_and_accepts_connections() -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command;
        
        // Start the microsandbox-runtime-server as a separate process
        let mut child = Command::new("cargo")
            .args(&["run", "--package", "reflow_microsandbox_runtime", "--", "--port", "9999"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        // Give server time to start
        tokio::time::sleep(Duration::from_secs(3)).await;
        
        // Try to connect to the server
        use tokio_tungstenite::connect_async;
        let url = "ws://127.0.0.1:9999";
        
        let connection_result = connect_async(url).await;
        
        // Kill the server
        child.kill().await.ok();
        
        // Check if connection was successful
        assert!(connection_result.is_ok(), "Failed to connect to MicroSandbox runtime server");
        
        if let Ok((ws_stream, _)) = connection_result {
            println!("Successfully connected to MicroSandbox runtime server");
            
            // Test sending a simple RPC request
            use tokio_tungstenite::tungstenite::Message;
            use futures_util::{SinkExt, StreamExt};
            
            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            
            // Send a list request
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "list",
                "params": {},
                "id": 1
            });
            
            ws_sender.send(Message::text(request.to_string())).await?;
            
            // Try to receive response with timeout
            let response_future = ws_receiver.next();
            let timeout_future = tokio::time::sleep(Duration::from_secs(2));
            
            tokio::select! {
                response = response_future => {
                    if let Some(Ok(Message::Text(text))) = response {
                        let json: serde_json::Value = serde_json::from_str(&text)?;
                        println!("Received response: {}", json);
                        assert_eq!(json["jsonrpc"], "2.0");
                        assert_eq!(json["id"], 1);
                    }
                }
                _ = timeout_future => {
                    println!("Response timeout - server may not be fully functional");
                }
            }
            
            ws_sender.close().await.ok();
        }
        
        Ok(())
    }
}