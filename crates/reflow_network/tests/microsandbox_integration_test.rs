#[cfg(test)]
mod microsandbox_integration_tests {
    use anyhow::Result;
    use tokio::sync::oneshot;

    // Helper function to start the microsandbox runtime server in background
    async fn start_runtime_server(
        port: u16,
    ) -> Result<(tokio::process::Child, oneshot::Sender<()>)> {
        use std::process::Stdio;
        use tokio::process::Command;

        println!("Building microsandbox-runtime-server...");
        // First build the server to ensure it's compiled
        let build_output = tokio::process::Command::new("cargo")
            .args(&["build", "--package", "reflow_microsandbox_runtime"])
            .output()
            .await?;

        if !build_output.status.success() {
            eprintln!("Failed to build microsandbox-runtime-server");
            return Err(anyhow::anyhow!("Build failed"));
        }

        println!("Starting microsandbox-runtime-server on port {}...", port);
        // Start the microsandbox-runtime-server as a separate process
        // Use the built binary directly to avoid cargo overhead
        let cwd = std::env::current_dir()?;
        println!("Current working directory: {:?}", cwd);
        let mut child = Command::new(format!(
            "{}/../../target/debug/microsandbox-runtime-server",
            cwd.display()
        ))
        .args(&["--port", &port.to_string()])
        .stdout(Stdio::piped()) // Capture stdout for debugging
        .stderr(Stdio::piped()) // Capture stderr for debugging
        .spawn()?;

        // Spawn a task to read and print the server's stdout
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("[SERVER STDOUT]: {}", line);
                }
            });
        }

        // Spawn a task to read and print the server's stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("[SERVER STDERR]: {}", line);
                }
            });
        }

        // Create a channel to signal shutdown (not used in this simpler version)
        let (shutdown_tx, _shutdown_rx) = oneshot::channel::<()>();

        // Give server time to start
        println!("Waiting for server to start...");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Verify the server is listening
        for attempt in 1..=5 {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(_) => {
                    println!("Server is listening on port {}", port);
                    break;
                }
                Err(_) if attempt < 5 => {
                    println!("Waiting for server... attempt {}/5", attempt);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Server failed to start: {}", e));
                }
            }
        }

        Ok((child, shutdown_tx))
    }

    #[tokio::test]
    #[ignore] // Requires MicroSandbox server to be available
    async fn test_microsandbox_runtime_server() -> Result<()> {
        // Start the server in background
        let (mut server_handle, shutdown_tx) = start_runtime_server(7777).await?;

        // Test WebSocket connection from host to sandbox runtime
        use tokio_tungstenite::connect_async;
        let url = "ws://127.0.0.1:7777";

        let res = connect_async(url).await;

        if res.is_err() {
            eprintln!(
                "Failed to connect to MicroSandbox runtime server: {}",
                res.err().unwrap()
            );
            shutdown_tx.send(()).ok();
            // server_handle.await.ok();
            return Err(anyhow::anyhow!(
                "Connection to MicroSandbox runtime server failed"
            ));
        }

        let (ws_stream, _) = res?;

        println!("Successfully connected to MicroSandbox runtime server");

        // Test RPC request to list actors
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send a list request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "list",
            "params": {},
            "id": "1"
        });

        println!("Sending request: {}", request.to_string());
        ws_sender.send(Message::text(request.to_string())).await?;
        println!("Request sent successfully");

        // Receive response with timeout
        println!("Waiting for response from server...");
        let response_future = ws_receiver.next();
        let timeout_future = tokio::time::sleep(tokio::time::Duration::from_secs(5));

        tokio::select! {
            response = response_future => {
                if let Some(Ok(Message::Text(response))) = response {
                    let response_json: serde_json::Value = serde_json::from_str(&response)?;
                    println!("Response: {}", response_json);
                    assert_eq!(response_json["jsonrpc"], "2.0");
                    assert_eq!(response_json["id"], "1");
                    assert!(response_json["result"]["actors"].is_array());
                } else {
                    println!("No response received or error in response");
                    return Err(anyhow::anyhow!("No valid response received"));
                }
            }
            _ = timeout_future => {
                println!("TIMEOUT: Server did not respond within 5 seconds");
                return Err(anyhow::anyhow!("Server response timeout"));
            }
        }

        // Close connection
        ws_sender.close().await?;

        // Cleanup - kill the server process
        server_handle.kill().await.ok();

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires MicroSandbox server to be available
    async fn test_actor_execution_via_websocket_rpc() -> Result<()> {
        // This test demonstrates the full flow:
        // 1. Host starts MicroSandboxRuntime (WebSocket RPC server)
        // 2. MicroSandboxRuntime uses MicroSandbox SDK to create sandboxes
        // 3. Host sends actor execution requests via WebSocket RPC
        // 4. Actors running in sandboxes communicate back via WebSocket RPC

        // Create a test Python actor script
        let python_actor = r#"
import json

# Actor decorator for metadata
def actor(metadata):
    def decorator(func):
        func.__actor_metadata__ = metadata
        return func
    return decorator

@actor({
    "component": "test_multiplier",
    "description": "Multiplies input by 2",
    "inports": ["input"],
    "outports": ["output"]
})
async def process(context):
    # Get input from the context (via WebSocket RPC)
    input_value = context.get_input("input", 0)
    
    # Process the value
    result = input_value * 2
    
    # Send output (via WebSocket RPC back to host)
    await context.send_output("output", result)
    
    return {"status": "completed"}
"#;

        // Save the actor script
        let actor_path = "/tmp/test_multiplier.py";
        std::fs::write(actor_path, python_actor)?;

        use futures_util::StreamExt;
        use futures_util::SinkExt;
        use tokio_tungstenite::tungstenite::Message;

        // Start the server in background
        let (mut server_handle, shutdown_tx) = start_runtime_server(7778).await?;

        // Test WebSocket connection from host to sandbox runtime
        use tokio_tungstenite::connect_async;
        let url = "ws://127.0.0.1:7778";

        let res = connect_async(url).await;

        if res.is_err() {
            eprintln!(
                "Failed to connect to MicroSandbox runtime server: {}",
                res.err().unwrap()
            );
            shutdown_tx.send(()).ok();
            // server_handle.await.ok();
            return Err(anyhow::anyhow!(
                "Connection to MicroSandbox runtime server failed"
            ));
        }

        let (ws_stream, _) = res?;

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // First, load the actor script
        let load_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "load",
            "params": {
                "file_path": actor_path
            },
            "id": "1"
        });

        println!("Loading actor from {}", actor_path);
        ws_sender.send(Message::text(load_request.to_string())).await?;

        // Wait for load response
        if let Some(Ok(Message::Text(response))) = ws_receiver.next().await {
            let response_json: serde_json::Value = serde_json::from_str(&response)?;
            println!("Load response: {}", response_json);
            assert_eq!(response_json["result"]["success"], true);
        }

        // Send process request
        // Note: The runtime currently hardcodes actor name as "test_actor"
        let process_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "process",
            "params": {
                "actor_id": "test_multiplier",
                "payload": {
                    "input": 21
                },
                "config": {},
                "state": {},
                "timestamp": chrono::Utc::now().timestamp_millis()
            },
            "id": "2"
        });

        ws_sender
            .send(Message::text(process_request.to_string()))
            .await?;

        // Handle responses and notifications
        let mut _received_output = false;
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(text) = msg {
                let json: serde_json::Value = serde_json::from_str(&text)?;

                if json["method"] == "script_output" {
                    // This is a notification from the actor running in the sandbox
                    println!("Received output notification: {}", json);
                    assert_eq!(json["params"]["data"], 42);
                    _received_output = true;
                } else if json["id"] == "2" {
                    // This is the response to our process request
                    println!("Process response: {}", json);
                    assert!(json["result"]["outputs"].is_object(), "Actor execution failed - MicroSandbox portal issue needs to be fixed");
                    break;
                }
            }
        }

        // Cleanup
        server_handle.kill().await.ok();
        std::fs::remove_file(actor_path).ok();

        Ok(())
    }
}
