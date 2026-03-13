/// Browser WebSocket RPC client for WASM targets.
///
/// Provides the same JSON-RPC 2.0 interface as `WebSocketRpcClient` but
/// uses `web-sys::WebSocket` and `web-sys::fetch` instead of tokio/tungstenite.
/// This allows reflow graphs with script components to deploy to and
/// communicate with dynASB directly from the browser.
use super::types::*;
use parking_lot::RwLock;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Deploy a script to dynASB from the browser via fetch.
///
/// Returns `(function_id, ws_url)` on success.
pub async fn browser_deploy_script(
    api_url: &str,
    name: &str,
    runtime: &str,
    code: &str,
    handler: &str,
    dependencies: Option<HashMap<String, String>>,
    timeout_seconds: Option<u32>,
) -> Result<(String, String), anyhow::Error> {
    let body = json!({
        "name": name,
        "runtime": runtime,
        "code": code,
        "handler": handler,
        "timeout_seconds": timeout_seconds.unwrap_or(300),
        "dependencies": dependencies,
    });

    let body_str = serde_json::to_string(&body)?;

    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));

    let headers = web_sys::Headers::new()
        .map_err(|e| anyhow::anyhow!("Failed to create headers: {:?}", e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| anyhow::anyhow!("Failed to set content-type: {:?}", e))?;
    opts.set_headers(&headers);

    let url = format!("{}/api/v1/functions", api_url);
    let request = web_sys::Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| anyhow::anyhow!("Failed to create request: {:?}", e))?;

    let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("No window object"))?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| anyhow::anyhow!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| anyhow::anyhow!("Response cast failed"))?;

    let json_value = JsFuture::from(
        resp.json()
            .map_err(|e| anyhow::anyhow!("Failed to get JSON: {:?}", e))?,
    )
    .await
    .map_err(|e| anyhow::anyhow!("JSON parse failed: {:?}", e))?;

    let deploy_resp: serde_json::Value = serde_wasm_bindgen::from_value(json_value)
        .map_err(|e| anyhow::anyhow!("Deserialize failed: {}", e))?;

    let success = deploy_resp
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !success {
        let error = deploy_resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(anyhow::anyhow!("Deployment failed: {}", error));
    }

    let function_id = deploy_resp
        .get("function_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No function_id in response"))?
        .to_string();

    // Derive WS URL from API URL (same host, /ws path)
    let ws_url = api_url
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let ws_endpoint = format!("{}/ws/{}", ws_url, function_id);

    Ok((function_id, ws_endpoint))
}

/// Browser-based WebSocket RPC client using web-sys.
///
/// Speaks the same JSON-RPC 2.0 protocol as the native `WebSocketRpcClient`.
pub struct BrowserRpcClient {
    url: String,
    ws: Arc<RwLock<Option<web_sys::WebSocket>>>,
    pending: Arc<RwLock<HashMap<String, flume::Sender<RpcResponse>>>>,
    output_sender: Arc<RwLock<Option<flume::Sender<ScriptOutput>>>>,
    request_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl BrowserRpcClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            ws: Arc::new(RwLock::new(None)),
            pending: Arc::new(RwLock::new(HashMap::new())),
            output_sender: Arc::new(RwLock::new(None)),
            request_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn set_output_channel(&self, sender: flume::Sender<ScriptOutput>) {
        *self.output_sender.write() = Some(sender);
    }

    /// Connect to the WebSocket endpoint.
    pub fn connect(&self) -> Result<(), anyhow::Error> {
        let ws = web_sys::WebSocket::new(&self.url)
            .map_err(|e| anyhow::anyhow!("WebSocket connect failed: {:?}", e))?;

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // Message handler
        let pending = self.pending.clone();
        let output_sender = self.output_sender.clone();
        let onmessage =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
                if let Some(text) = e.data().as_string() {
                    // Try to parse as RPC response or notification
                    if let Ok(msg) = serde_json::from_str::<WebSocketMessage>(&text) {
                        match msg {
                            WebSocketMessage::Response(resp) => {
                                if let Some(tx) = pending.write().remove(&resp.id) {
                                    let _ = tx.send(resp);
                                }
                            }
                            WebSocketMessage::Notification(notif) => {
                                if notif.method == "output" || notif.method == "script_output" {
                                    if let Some(sender) = &*output_sender.read() {
                                        if let Ok(output) =
                                            serde_json::from_value::<ScriptOutput>(notif.params)
                                        {
                                            let _ = sender.send(output);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget(); // prevent closure from being dropped

        *self.ws.write() = Some(ws);
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, anyhow::Error> {
        let ws = self
            .ws
            .read()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("WebSocket not connected"))?;

        let id = self
            .request_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_string();

        let request = RpcRequest::new(id.clone(), method.to_string(), params);
        let msg = serde_json::to_string(&request)?;

        // Register pending response channel
        let (tx, rx) = flume::bounded(1);
        self.pending.write().insert(id.clone(), tx);

        // Send
        ws.send_with_str(&msg)
            .map_err(|e| anyhow::anyhow!("WebSocket send failed: {:?}", e))?;

        // Await response
        let response = rx
            .recv_async()
            .await
            .map_err(|_| anyhow::anyhow!("Response channel closed for request {}", id))?;

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "RPC error {}: {}",
                error.code,
                error.message
            ));
        }

        Ok(response.result.unwrap_or(Value::Null))
    }
}
