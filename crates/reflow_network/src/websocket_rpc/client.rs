use super::types::*;
use anyhow::{Result, Context};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream, tungstenite::Message};
use uuid::Uuid;
use tracing::{debug, warn, error};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// WebSocket RPC client for communicating with script actors
pub struct WebSocketRpcClient {
    url: String,
    ws: Arc<Mutex<Option<WsStream>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    running: Arc<Mutex<bool>>,
    /// Channel for receiving async outputs from scripts
    pub output_sender: Arc<RwLock<Option<flume::Sender<ScriptOutput>>>>,
}

impl WebSocketRpcClient {
    /// Create a new WebSocket RPC client
    pub fn new(url: String) -> Self {
        Self {
            url,
            ws: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(Mutex::new(false)),
            output_sender: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Set the output channel for async messages from scripts
    pub fn set_output_channel(&self, sender: flume::Sender<ScriptOutput>) {
        *self.output_sender.write() = Some(sender);
    }
    
    /// Connect to the WebSocket server
    pub async fn connect(&self) -> Result<()> {
        debug!("Connecting to WebSocket RPC server: {}", self.url);
        println!("DEBUG: connect() called for URL: {}", self.url);
        
        // Check if already connected
        if self.is_connected().await {
            println!("DEBUG: Already connected, skipping reconnection");
            return Ok(());
        }
        
        let (ws_stream, _) = connect_async(&self.url)
            .await
            .context("Failed to connect to WebSocket server")?;
        
        *self.ws.lock().await = Some(ws_stream);
        *self.running.lock().await = true;
        
        // Start message handler
        self.start_handler().await;
        
        debug!("Connected to WebSocket RPC server");
        println!("DEBUG: Successfully connected and handler started");
        Ok(())
    }
    
    /// Disconnect from the server
    pub async fn disconnect(&self) -> Result<()> {
        *self.running.lock().await = false;
        
        if let Some(mut ws) = self.ws.lock().await.take() {
            ws.close(None).await?;
        }
        
        // Clear pending requests
        let mut pending = self.pending.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Value::Null);
        }
        
        Ok(())
    }
    
    /// Make an RPC call
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        println!("DEBUG: call() invoked with method: {}", method);
        let id = Uuid::new_v4().to_string();
        
        let request = RpcRequest::new(id.clone(), method.to_string(), params);
        let message = serde_json::to_string(&request)?;
        println!("DEBUG: Sending RPC request: {}", message);
        
        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        
        // Send message
        let mut ws_guard = self.ws.lock().await;
        if let Some(ws) = ws_guard.as_mut() {
            println!("DEBUG: Sending message via WebSocket");
            ws.send(Message::text(message)).await?;
            println!("DEBUG: Message sent successfully");
        } else {
            println!("DEBUG: WebSocket not connected!");
            return Err(anyhow::anyhow!("Not connected to WebSocket server"));
        }
        drop(ws_guard);
        
        println!("DEBUG: Waiting for response with timeout...");
        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => {
                println!("DEBUG: Got response!");
                Ok(response)
            },
            Ok(Err(_)) => {
                println!("DEBUG: RPC call cancelled");
                Err(anyhow::anyhow!("RPC call cancelled"))
            },
            Err(_) => {
                println!("DEBUG: RPC call timed out after 30 seconds");
                self.pending.lock().await.remove(&id);
                Err(anyhow::anyhow!("RPC call timed out"))
            }
        }
    }
    
    /// Start the message handler
    async fn start_handler(&self) {
        let ws = self.ws.clone();
        let pending = self.pending.clone();
        let running = self.running.clone();
        let output_sender = self.output_sender.clone();
        
        tokio::spawn(async move {
            while *running.lock().await {
                // Use select with timeout to avoid holding lock indefinitely
                let msg = {
                    let mut ws_guard = ws.lock().await;
                    if let Some(ws_stream) = ws_guard.as_mut() {
                        // Use timeout to avoid blocking forever
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(100),
                            ws_stream.next()
                        ).await {
                            Ok(Some(msg)) => Some(msg),
                            Ok(None) => None,
                            Err(_) => None, // Timeout, will continue in loop
                        }
                    } else {
                        None
                    }
                }; // Lock released here
                
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let pending_clone = pending.clone();
                        let output_sender_clone = output_sender.clone();
                        WebSocketRpcClient::handle_message(
                            text.to_string(), 
                            &pending_clone,
                            output_sender_clone
                        ).await;
                    }
                    Some(Ok(Message::Close(_))) => {
                        warn!("WebSocket connection closed");
                        *running.lock().await = false;
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        *running.lock().await = false;
                        break;
                    }
                    None => {
                        // No connection or no message, wait a bit
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                    _ => {}
                }
            }
        });
    }
    
    /// Handle incoming message
    async fn handle_message(
        text: String,
        pending: &Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
        output_sender: Arc<RwLock<Option<flume::Sender<ScriptOutput>>>>
    ) {
        // Try to parse as WebSocketMessage (either Response or Notification)
        match serde_json::from_str::<WebSocketMessage>(&text) {
            Ok(WebSocketMessage::Response(response)) => {
                if let Some(tx) = pending.lock().await.remove(&response.id) {
                    if let Some(result) = response.result {
                        let _ = tx.send(result);
                    } else if let Some(error) = response.error {
                        let _ = tx.send(json!({
                            "error": {
                                "code": error.code,
                                "message": error.message,
                                "data": error.data
                            }
                        }));
                    } else {
                        let _ = tx.send(Value::Null);
                    }
                }
            }
            Ok(WebSocketMessage::Notification(notification)) => {
                // Handle async notification from script
                // Support both "output" (from MicroSandbox) and "script_output" (from tests/bridge)
                if notification.method == "output" || notification.method == "script_output" {
                    if let Some(sender) = &*output_sender.read() {
                        // Parse the output data - handle both direct ScriptOutput and nested params
                        let output_data = if notification.method == "script_output" {
                            // For script_output, the ScriptOutput fields are directly in params
                            notification.params
                        } else {
                            // For output, it might be wrapped
                            notification.params
                        };
                        
                        if let Ok(output) = serde_json::from_value::<ScriptOutput>(output_data) {
                            debug!("Received async output from script: {} on port {}", 
                                output.actor_id, output.port);
                            let _ = sender.send(output);
                        }
                    }
                } else if notification.method == "log" {
                    // Handle log messages from scripts
                    if let Some(log_msg) = notification.params.as_str() {
                        debug!("Script log: {}", log_msg);
                    }
                } else if notification.method == "state_update" {
                    // Handle state updates from scripts
                    debug!("Script state update: {:?}", notification.params);
                    // TODO: Apply state updates via Redis
                }
            }
            Err(e) => {
                warn!("Failed to parse WebSocket message: {}", e);
            }
        }
    }
    
    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        println!("DEBUG: is_connected - acquiring running lock");
        let running = *self.running.lock().await;
        println!("DEBUG: is_connected - running = {}", running);
        println!("DEBUG: is_connected - acquiring ws lock");
        let has_ws = self.ws.lock().await.is_some();
        println!("DEBUG: is_connected - has_ws = {}", has_ws);
        running && has_ws
    }
    
    /// Reconnect if disconnected
    pub async fn ensure_connected(&self) -> Result<()> {
        println!("DEBUG: ensure_connected - checking connection");
        let connected = self.is_connected().await;
        println!("DEBUG: is_connected = {}", connected);
        if !connected {
            println!("DEBUG: Not connected, attempting to connect...");
            self.connect().await?;
            println!("DEBUG: Connected successfully");
        }
        Ok(())
    }
}

impl Drop for WebSocketRpcClient {
    fn drop(&mut self) {
        // Try to disconnect gracefully
        let ws = self.ws.clone();
        let running = self.running.clone();
        
        tokio::spawn(async move {
            *running.lock().await = false;
            if let Some(mut ws_stream) = ws.lock().await.take() {
                let _ = ws_stream.close(None).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_rpc_request_serialization() {
        let request = RpcRequest::new(
            "test-id".to_string(),
            "process".to_string(),
            json!({"foo": "bar"})
        );
        
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"jsonrpc\":\"2.0\""));
        assert!(serialized.contains("\"id\":\"test-id\""));
        assert!(serialized.contains("\"method\":\"process\""));
    }
    
    #[tokio::test]
    async fn test_rpc_response_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": "test-id",
            "result": {"status": "ok"}
        }"#;
        
        let response: RpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "test-id");
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }
}