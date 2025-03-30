use boa_engine::object::FunctionObjectBuilder;
use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer, native_function::NativeFunction};
use crate::security::PermissionManager;
use crate::networking::NetworkConfig;
use crate::state::{GlobalState, store_js_value, get_js_value, remove_js_value};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketState {
    /// Connecting
    Connecting,
    
    /// Open
    Open,
    
    /// Closing
    Closing,
    
    /// Closed
    Closed,
}

/// WebSocket connection
pub struct WebSocketConnection {
    /// URL
    url: String,
    
    /// State
    state: WebSocketState,
    
    /// Protocols
    protocols: Vec<String>,
}

impl WebSocketConnection {
    /// Create a new WebSocket connection
    pub fn new(url: String, protocols: Vec<String>) -> Self {
        WebSocketConnection {
            url,
            state: WebSocketState::Connecting,
            protocols,
        }
    }
    
    /// Get the URL
    pub fn url(&self) -> &str {
        &self.url
    }
    
    /// Get the state
    pub fn state(&self) -> WebSocketState {
        self.state
    }
    
    /// Get the protocols
    pub fn protocols(&self) -> &[String] {
        &self.protocols
    }
    
    /// Set the state
    pub fn set_state(&mut self, state: WebSocketState) {
        self.state = state;
    }
}

/// WebSocket module
pub struct WebSocketModule {
    /// Permission manager
    permissions: PermissionManager,
    
    /// Network configuration
    config: NetworkConfig,
    
    /// Active connections
    connections: Arc<Mutex<HashMap<u32, WebSocketConnection>>>,
    
    /// Next connection ID
    next_connection_id: Arc<Mutex<u32>>,
    
    /// Global state
    state: GlobalState,
}

impl WebSocketModule {
    /// Create a new WebSocket module
    pub fn new(permissions: PermissionManager, config: NetworkConfig) -> Self {
        WebSocketModule {
            permissions,
            config,
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_connection_id: Arc::new(Mutex::new(1)),
            state: GlobalState::new(),
        }
    }
    
    /// Register the WebSocket module with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Create the WebSocket constructor
        let websocket_constructor = FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::constructor)).build();
        
        // Create the WebSocket prototype
        let websocket_prototype = ObjectInitializer::new(context).build();
        
        // Add the WebSocket prototype methods
        self.register_prototype_methods(context, &websocket_prototype.clone().into())?;
        
        // Set the prototype for the constructor
        websocket_constructor.set("prototype", websocket_prototype, true, context)?;
        
        // Add the WebSocket constants
        websocket_constructor.set("CONNECTING", 0, true, context)?;
        websocket_constructor.set("OPEN", 1, true, context)?;
        websocket_constructor.set("CLOSING", 2, true, context)?;
        websocket_constructor.set("CLOSED", 3, true, context)?;
        
        // Add the WebSocket constructor to the global object
        let global = context.global_object();
        global.set("WebSocket", websocket_constructor, true, context)?;
        
        // Store the WebSocket module in the global state
        let module_arc = Arc::new(self.clone());
        self.state.set(module_arc);
        
        Ok(())
    }
    
    /// Register the WebSocket prototype methods
    fn register_prototype_methods(&self, context: &mut Context, prototype: &JsValue) -> JsResult<()> {
        // Add the close method
        if let Some(prototype_obj) = prototype.as_object() {
            prototype_obj.set(
                "close",
                FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::close)).build(),
                true,
                context,
            )?;
            
            // Add the send method
            prototype_obj.set(
                "send",
                FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::send)).build(),
                true,
                context,
            )?;
        }
        
        Ok(())
    }
    
    /// WebSocket constructor
    fn constructor(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        // Check arguments
        if args.is_empty() {
            return Err(JsError::from_opaque("WebSocket constructor requires a URL argument".into()));
        }
        
        // Get the URL
        let url = args[0].to_string(ctx)?.to_std_string_escaped();
        
        // Get the protocols (optional)
        let protocols = if args.len() > 1 {
            if args[1].is_string() {
                vec![args[1].to_string(ctx)?.to_std_string_escaped()]
            } else if let Some(array) = args[1].as_object() {
                // Check if it has a length property, which is a good indicator it's an array
                if let Ok(length_val) = array.get("length", ctx) {
                    if let Ok(length) = length_val.to_number(ctx) {
                        let length = length as usize;
                        let mut protocols = Vec::with_capacity(length);
                        
                        for i in 0..length {
                            if let Ok(protocol_val) = array.get(i as u32, ctx) {
                                if let Ok(protocol) = protocol_val.to_string(ctx) {
                                    protocols.push(protocol.to_std_string_escaped());
                                }
                            }
                        }
                        
                        protocols
                    } else {
                        return Err(JsError::from_opaque("WebSocket protocols array length must be a number".into()));
                    }
                } else {
                    return Err(JsError::from_opaque("WebSocket protocols must be a string or an array of strings".into()));
                }
            } else {
                return Err(JsError::from_opaque("WebSocket protocols must be a string or an array of strings".into()));
            }
        } else {
            Vec::new()
        };
        
        // Create a WebSocket object
        let websocket_obj = if let Some(this_obj) = this.as_object() {
            this_obj.clone()
        } else {
            ObjectInitializer::new(ctx).build()
        };
        
        // Add the WebSocket properties
        websocket_obj.set("url", url.clone(), true, ctx)?;
        websocket_obj.set("readyState", 0, true, ctx)?;
        websocket_obj.set("protocol", "", true, ctx)?;
        websocket_obj.set("extensions", "", true, ctx)?;
        websocket_obj.set("binaryType", "blob", true, ctx)?;
        
        // Add the WebSocket event handlers
        websocket_obj.set("onopen", JsValue::null(), true, ctx)?;
        websocket_obj.set("onmessage", JsValue::null(), true, ctx)?;
        websocket_obj.set("onerror", JsValue::null(), true, ctx)?;
        websocket_obj.set("onclose", JsValue::null(), true, ctx)?;
        
        // Create a new WebSocket connection
        let connection = WebSocketConnection::new(url, protocols);
        
        // Get the WebSocket module from the global state
        let state = GlobalState::new();
        let websocket_module = state.get::<Arc<WebSocketModule>>().unwrap();
        
        // Add the connection to the connections map
        let connection_id = {
            let mut next_connection_id = websocket_module.next_connection_id.lock().unwrap();
            let id = *next_connection_id;
            *next_connection_id = next_connection_id.wrapping_add(1);
            id
        };
        
        let mut connections = websocket_module.connections.lock().unwrap();
        connections.insert(connection_id, connection);
        
        // Store the connection ID in thread-local storage
        store_js_value(connection_id, JsValue::from(connection_id));
        
        // Store the connection ID in the WebSocket object
        websocket_obj.set("__connection_id", connection_id, true, ctx)?;
        
        // Simulate a connection
        // In a real implementation, this would be done asynchronously
        // For now, we'll just simulate a successful connection
        websocket_obj.set("readyState", 1, true, ctx)?;
        
        // Call the onopen handler if it exists
        let onopen = websocket_obj.get("onopen", ctx)?;
        if !onopen.is_null() && !onopen.is_undefined() {
            if let Some(onopen_obj) = onopen.as_object() {
                let event = ObjectInitializer::new(ctx).build();
                event.set("type", "open", true, ctx)?;
                event.set("target", websocket_obj.clone(), true, ctx)?;
                
                let args = [event.into()];
                if let Err(err) = onopen_obj.call(&JsValue::undefined(), &args, ctx) {
                    // Log the error
                    eprintln!("Error calling onopen handler: {}", err);
                }
            }
        }
        
        Ok(websocket_obj.into())
    }
    
    /// Close method
    fn close(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        // Get the WebSocket object
        let websocket_obj = if let Some(this_obj) = this.as_object() {
            this_obj.clone()
        } else {
            return Err(JsError::from_opaque("this is not an object".into()));
        };
        
        // Get the connection ID
        let connection_id = websocket_obj.get("__connection_id", ctx)?.to_number(ctx)? as u32;
        
        // Get the WebSocket module from the global state
        let state = GlobalState::new();
        let websocket_module = state.get::<Arc<WebSocketModule>>().unwrap();
        
        // Get the connection
        let mut connections = websocket_module.connections.lock().unwrap();
        if let Some(connection) = connections.get_mut(&connection_id) {
            // Set the connection state to closing
            connection.set_state(WebSocketState::Closing);
            
            // Update the readyState property
            websocket_obj.set("readyState", 2, true, ctx)?;
            
            // Simulate a close
            // In a real implementation, this would be done asynchronously
            // For now, we'll just simulate a successful close
            connection.set_state(WebSocketState::Closed);
            websocket_obj.set("readyState", 3, true, ctx)?;
            
            // Call the onclose handler if it exists
            let onclose = websocket_obj.get("onclose", ctx)?;
            if !onclose.is_null() && !onclose.is_undefined() {
                if let Some(onclose_obj) = onclose.as_object() {
                    let event = ObjectInitializer::new(ctx).build();
                    event.set("type", "close", true, ctx)?;
                    event.set("target", websocket_obj.clone(), true, ctx)?;
                    event.set("code", 1000, true, ctx)?;
                    event.set("reason", "", true, ctx)?;
                    event.set("wasClean", true, true, ctx)?;
                    
                    let args = [event.into()];
                    if let Err(err) = onclose_obj.call(&JsValue::undefined(), &args, ctx) {
                        // Log the error
                        eprintln!("Error calling onclose handler: {}", err);
                    }
                }
            }
            
            // Remove the connection from the connections map
            connections.remove(&connection_id);
            
            // Remove the connection ID from thread-local storage
            remove_js_value(connection_id);
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Send method
    fn send(this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        // Check arguments
        if args.is_empty() {
            return Err(JsError::from_opaque("send requires a data argument".into()));
        }
        
        // Get the WebSocket object
        let websocket_obj = if let Some(this_obj) = this.as_object() {
            this_obj.clone()
        } else {
            return Err(JsError::from_opaque("this is not an object".into()));
        };
        
        // Get the readyState property
        let ready_state = websocket_obj.get("readyState", ctx)?.to_number(ctx)? as u32;
        
        // Check if the WebSocket is open
        if ready_state != 1 {
            return Err(JsError::from_opaque("WebSocket is not open".into()));
        }
        
        // Get the connection ID
        let connection_id = websocket_obj.get("__connection_id", ctx)?.to_number(ctx)? as u32;
        
        // Get the WebSocket module from the global state
        let state = GlobalState::new();
        let websocket_module = state.get::<Arc<WebSocketModule>>().unwrap();
        
        // Get the connection
        let connections = websocket_module.connections.lock().unwrap();
        if let Some(connection) = connections.get(&connection_id) {
            // Get the data
            let data = args[0].clone();
            
            // In a real implementation, this would send the data to the server
            // For now, we'll just simulate a successful send
            
            // Simulate a message
            // In a real implementation, this would be done asynchronously
            // For now, we'll just simulate a message from the server
            let onmessage = websocket_obj.get("onmessage", ctx)?;
            if !onmessage.is_null() && !onmessage.is_undefined() {
                if let Some(onmessage_obj) = onmessage.as_object() {
                    let event = ObjectInitializer::new(ctx).build();
                    event.set("type", "message", true, ctx)?;
                    event.set("target", websocket_obj.clone(), true, ctx)?;
                    event.set("data", "Echo: ".to_string() + &data.to_string(ctx)?.to_std_string_escaped(), true, ctx)?;
                    
                    let args = [event.into()];
                    if let Err(err) = onmessage_obj.call(&JsValue::undefined(), &args, ctx) {
                        // Log the error
                        eprintln!("Error calling onmessage handler: {}", err);
                    }
                }
            }
        }
        
        Ok(JsValue::undefined())
    }
}

impl Clone for WebSocketModule {
    fn clone(&self) -> Self {
        WebSocketModule {
            permissions: self.permissions.clone(),
            config: self.config.clone(),
            connections: self.connections.clone(),
            next_connection_id: self.next_connection_id.clone(),
            state: self.state.clone(),
        }
    }
}

impl From<WebSocketModule> for JsValue {
    fn from(module: WebSocketModule) -> Self {
        // Create a constructor function that calls the WebSocket constructor
        let mut ctx = Context::default();
        let constructor = FunctionObjectBuilder::new(&mut ctx, NativeFunction::from_fn_ptr(WebSocketModule::constructor)).build();
        constructor.into()
    }
}
