use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer, property::Attribute};
use crate::runtime::{Extension, ExtensionError};
use crate::security::PermissionManager;
use crate::async_runtime::{AsyncRuntime, Task};
use crate::networking::NetworkConfig;
use crate::networking::is_host_allowed;
use async_trait::async_trait;
use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};
use url::Url;

/// WebSocket connection
pub struct WebSocketConnection {
    /// Connection ID
    id: u32,
    
    /// URL
    url: String,
    
    /// Message sender
    sender: Option<Sender<Message>>,
    
    /// Ready state
    ready_state: u8,
    
    /// Event handlers
    event_handlers: HashMap<String, Vec<JsValue>>,
    
    /// Async runtime
    async_runtime: Arc<AsyncRuntime>,
}

impl WebSocketConnection {
    /// Create a new WebSocket connection
    pub fn new(id: u32, url: String, async_runtime: Arc<AsyncRuntime>) -> Self {
        WebSocketConnection {
            id,
            url,
            sender: None,
            ready_state: 0, // CONNECTING
            event_handlers: HashMap::new(),
            async_runtime,
        }
    }
    
    /// Get the connection ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// Get the URL
    pub fn url(&self) -> &str {
        &self.url
    }
    
    /// Get the ready state
    pub fn ready_state(&self) -> u8 {
        self.ready_state
    }
    
    /// Set the ready state
    pub fn set_ready_state(&mut self, ready_state: u8) {
        self.ready_state = ready_state;
    }
    
    /// Set the message sender
    pub fn set_sender(&mut self, sender: Sender<Message>) {
        self.sender = Some(sender);
    }
    
    /// Add an event handler
    pub fn add_event_handler(&mut self, event: &str, handler: JsValue) {
        let handlers = self.event_handlers.entry(event.to_string()).or_insert_with(Vec::new);
        handlers.push(handler);
    }
    
    /// Remove an event handler
    pub fn remove_event_handler(&mut self, event: &str, handler: &JsValue) {
        if let Some(handlers) = self.event_handlers.get_mut(event) {
            handlers.retain(|h| h != handler);
        }
    }
    
    /// Get event handlers
    pub fn get_event_handlers(&self, event: &str) -> Vec<JsValue> {
        self.event_handlers.get(event).cloned().unwrap_or_default()
    }
    
    /// Send a message
    pub async fn send(&self, message: &str) -> Result<(), JsError> {
        if self.ready_state != 1 {
            return Err(JsError::from_opaque("WebSocket is not open".into()));
        }
        
        if let Some(sender) = &self.sender {
            sender.send(Message::Text(message.to_string())).await
                .map_err(|e| JsError::from_opaque(format!("Failed to send message: {}", e).into()))?;
        } else {
            return Err(JsError::from_opaque("WebSocket is not connected".into()));
        }
        
        Ok(())
    }
    
    /// Close the connection
    pub async fn close(&mut self, code: Option<u16>, reason: Option<&str>) -> Result<(), JsError> {
        if self.ready_state == 2 || self.ready_state == 3 {
            return Ok(());
        }
        
        self.ready_state = 2; // CLOSING
        
        if let Some(sender) = &self.sender {
            let message = match (code, reason) {
                (Some(code), Some(reason)) => Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: code.into(),
                    reason: reason.to_string().into(),
                })),
                (Some(code), None) => Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: code.into(),
                    reason: "".into(),
                })),
                _ => Message::Close(None),
            };
            
            sender.send(message).await
                .map_err(|e| JsError::from_opaque(format!("Failed to close connection: {}", e).into()))?;
        }
        
        self.sender = None;
        self.ready_state = 3; // CLOSED
        
        Ok(())
    }
}

/// WebSocket module
#[derive(Debug)]
pub struct WebSocketModule {
    /// Network configuration
    config: NetworkConfig,
    
    /// WebSocket connections
    connections: Arc<RwLock<HashMap<u32, Arc<Mutex<WebSocketConnection>>>>>,
    
    /// Next connection ID
    next_id: Arc<Mutex<u32>>,
    
    /// Async runtime
    async_runtime: Option<Arc<AsyncRuntime>>,
}

impl WebSocketModule {
    /// Create a new WebSocket module
    pub fn new(config: NetworkConfig) -> Self {
        WebSocketModule {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
            async_runtime: None,
        }
    }
    
    /// Set the async runtime
    pub fn set_async_runtime(&mut self, async_runtime: Arc<AsyncRuntime>) {
        self.async_runtime = Some(async_runtime);
    }
    
    /// Get the next connection ID
    fn next_id(&self) -> u32 {
        let mut id = self.next_id.lock().unwrap();
        let current = *id;
        *id = id.wrapping_add(1);
        current
    }
    
    /// Check if a URL is allowed
    fn check_url(&self, url: &Url) -> Result<(), JsError> {
        // Check if the host is allowed
        if let Some(host) = url.host_str() {
            if !is_host_allowed(host, &self.config) {
                return Err(JsError::from_opaque(format!("Access to host '{}' is not allowed", host).into()));
            }
        }
        
        Ok(())
    }
    
    /// Create a WebSocket
    fn create_websocket(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // Check if we have at least one argument
        if args.is_empty() {
            return Err(JsError::from_opaque("WebSocket constructor requires a URL").into());
        }
        
        // Get the URL
        let url = args[0].to_string(context)?;
        
        // Parse the URL
        let parsed_url = Url::parse(&url).map_err(|e| JsError::from_opaque(format!("Invalid URL: {}", e).into()))?;
        
        // Check if the URL is allowed
        self.check_url(&parsed_url)?;
        
        // Get the async runtime
        let async_runtime = match &self.async_runtime {
            Some(runtime) => runtime.clone(),
            None => return Err(JsError::from_opaque("Async runtime not available".into())),
        };
        
        // Create the WebSocket object
        let ws_obj = context.create_object();
        
        // Create the WebSocket connection
        let id = self.next_id();
        let connection = WebSocketConnection::new(id, url.clone(), async_runtime.clone());
        let connection = Arc::new(Mutex::new(connection));
        
        // Store the connection
        {
            let mut connections = self.connections.write().unwrap();
            connections.insert(id, connection.clone());
        }
        
        // Store the connection ID in the WebSocket object
        ws_obj.set("__id", id, true, context)?;
        
        // Set the URL property
        ws_obj.set("url", url, true, context)?;
        
        // Set the ready state property
        ws_obj.set("readyState", 0, true, context)?;
        
        // Create the send method
        let send_fn = {
            let connection = connection.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                if args.is_empty() {
                    return Err(JsError::from_opaque("send requires a message").into());
                }
                
                let message = args[0].to_string(ctx)?;
                
                let connection_clone = connection.clone();
                let async_runtime = async_runtime.clone();
                
                // Create a task to send the message
                let task = Task::new(move |_ctx| {
                    // Send the message
                    let connection = connection_clone.lock().unwrap();
                    
                    tokio::spawn(async move {
                        if let Err(e) = connection.send(&message).await {
                            // TODO: Dispatch error event
                            eprintln!("Failed to send message: {}", e);
                        }
                    });
                    
                    Ok(JsValue::undefined())
                });
                
                // Schedule the task
                async_runtime.schedule_task(task);
                
                Ok(JsValue::undefined())
            }
        };
        
        // Create the close method
        let close_fn = {
            let connection = connection.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                let code = if args.len() > 0 {
                    Some(args[0].to_number(ctx)? as u16)
                } else {
                    None
                };
                
                let reason = if args.len() > 1 {
                    Some(args[1].to_string(ctx)?)
                } else {
                    None
                };
                
                let connection_clone = connection.clone();
                let async_runtime = async_runtime.clone();
                
                // Create a task to close the connection
                let task = Task::new(move |_ctx| {
                    // Close the connection
                    let mut connection = connection_clone.lock().unwrap();
                    
                    tokio::spawn(async move {
                        if let Err(e) = connection.close(code, reason.as_deref()).await {
                            // TODO: Dispatch error event
                            eprintln!("Failed to close connection: {}", e);
                        }
                    });
                    
                    Ok(JsValue::undefined())
                });
                
                // Schedule the task
                async_runtime.schedule_task(task);
                
                Ok(JsValue::undefined())
            }
        };
        
        // Create the addEventListener method
        let add_event_listener_fn = {
            let connection = connection.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                if args.len() < 2 {
                    return Err(JsError::from_opaque("addEventListener requires an event name and a listener".into()));
                }
                
                let event = args[0].to_string(ctx)?;
                let listener = args[1].clone();
                
                if !listener.is_function() {
                    return Err(JsError::from_opaque("Listener must be a function".into()));
                }
                
                // Add the event listener
                let mut connection = connection.lock().unwrap();
                connection.add_event_handler(&event, listener);
                
                Ok(JsValue::undefined())
            }
        };
        
        // Create the removeEventListener method
        let remove_event_listener_fn = {
            let connection = connection.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                if args.len() < 2 {
                    return Err(JsError::from_opaque("removeEventListener requires an event name and a listener".into()));
                }
                
                let event = args[0].to_string(ctx)?;
                let listener = &args[1];
                
                // Remove the event listener
                let mut connection = connection.lock().unwrap();
                connection.remove_event_handler(&event, listener);
                
                Ok(JsValue::undefined())
            }
        };
        
        // Add the methods to the WebSocket object
        ws_obj.set("send", send_fn, true, context)?;
        ws_obj.set("close", close_fn, true, context)?;
        ws_obj.set("addEventListener", add_event_listener_fn, true, context)?;
        ws_obj.set("removeEventListener", remove_event_listener_fn, true, context)?;
        
        // Create a task to connect to the WebSocket
        let task = Task::new(move |ctx| {
            // Connect to the WebSocket
            let connection_clone = connection.clone();
            let async_runtime = async_runtime.clone();
            
            tokio::spawn(async move {
                // Connect to the WebSocket
                match connect_async(url).await {
                    Ok((ws_stream, _)) => {
                        // Update the ready state
                        {
                            let mut connection = connection_clone.lock().unwrap();
                            connection.set_ready_state(1); // OPEN
                            
                            // Create a task to dispatch the open event
                            let connection_clone = connection_clone.clone();
                            let task = Task::new(move |ctx| {
                                // Get the event handlers
                                let connection = connection_clone.lock().unwrap();
                                let handlers = connection.get_event_handlers("open");
                                
                                // Create the event object
                                let event_obj = ctx.create_object();
                                event_obj.set("type", "open", true, ctx)?;
                                
                                // Call the event handlers
                                for handler in handlers {
                                    if handler.is_function() {
                                        let _ = ctx.call(&handler, &JsValue::undefined(), &[event_obj.clone().into()]);
                                    }
                                }
                                
                                // Update the readyState property
                                let ws_obj = ctx.global_object().get_property("WebSocket", ctx)?;
                                if let Ok(instances) = ws_obj.get_property("instances", ctx) {
                                    if let Ok(instance) = instances.get_property(&connection.id().to_string(), ctx) {
                                        instance.set("readyState", 1, true, ctx)?;
                                    }
                                }
                                
                                Ok(JsValue::undefined())
                            });
                            
                            // Schedule the task
                            async_runtime.schedule_task(task);
                        }
                        
                        // Split the WebSocket stream
                        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                        
                        // Create a channel for sending messages
                        let (sender, mut receiver) = mpsc::channel::<Message>(32);
                        
                        // Store the sender
                        {
                            let mut connection = connection_clone.lock().unwrap();
                            connection.set_sender(sender);
                        }
                        
                        // Spawn a task to forward messages to the WebSocket
                        tokio::spawn(async move {
                            while let Some(message) = receiver.recv().await {
                                if let Err(e) = ws_sender.send(message).await {
                                    eprintln!("Failed to send message: {}", e);
                                    break;
                                }
                            }
                        });
                        
                        // Process incoming messages
                        while let Some(message) = ws_receiver.next().await {
                            match message {
                                Ok(message) => {
                                    // Create a task to dispatch the message event
                                    let connection_clone = connection_clone.clone();
                                    let message_text = match &message {
                                        Message::Text(text) => text.clone(),
                                        Message::Binary(data) => String::from_utf8_lossy(data).to_string(),
                                        _ => continue,
                                    };
                                    
                                    let task = Task::new(move |ctx| {
                                        // Get the event handlers
                                        let connection = connection_clone.lock().unwrap();
                                        let handlers = connection.get_event_handlers("message");
                                        
                                        // Create the event object
                                        let event_obj = ctx.create_object();
                                        event_obj.set("type", "message", true, ctx)?;
                                        event_obj.set("data", message_text, true, ctx)?;
                                        
                                        // Call the event handlers
                                        for handler in handlers {
                                            if handler.is_function() {
                                                let _ = ctx.call(&handler, &JsValue::undefined(), &[event_obj.clone().into()]);
                                            }
                                        }
                                        
                                        Ok(JsValue::undefined())
                                    });
                                    
                                    // Schedule the task
                                    async_runtime.schedule_task(task);
                                },
                                Err(e) => {
                                    // Create a task to dispatch the error event
                                    let connection_clone = connection_clone.clone();
                                    let error_message = e.to_string();
                                    
                                    let task = Task::new(move |ctx| {
                                        // Get the event handlers
                                        let connection = connection_clone.lock().unwrap();
                                        let handlers = connection.get_event_handlers("error");
                                        
                                        // Create the event object
                                        let event_obj = ctx.create_object();
                                        event_obj.set("type", "error", true, ctx)?;
                                        event_obj.set("message", error_message, true, ctx)?;
                                        
                                        // Call the event handlers
                                        for handler in handlers {
                                            if handler.is_function() {
                                                let _ = ctx.call(&handler, &JsValue::undefined(), &[event_obj.clone().into()]);
                                            }
                                        }
                                        
                                        Ok(JsValue::undefined())
                                    });
                                    
                                    // Schedule the task
                                    async_runtime.schedule_task(task);
                                    
                                    break;
                                },
                            }
                        }
                        
                        // Create a task to dispatch the close event
                        let connection_clone = connection_clone.clone();
                        let task = Task::new(move |ctx| {
                            // Update the ready state
                            {
                                let mut connection = connection_clone.lock().unwrap();
                                connection.set_ready_state(3); // CLOSED
                            }
                            
                            // Get the event handlers
                            let connection = connection_clone.lock().unwrap();
                            let handlers = connection.get_event_handlers("close");
                            
                            // Create the event object
                            let event_obj = ctx.create_object();
                            event_obj.set("type", "close", true, ctx)?;
                            
                            // Call the event handlers
                            for handler in handlers {
                                if handler.is_function() {
                                    let _ = ctx.call(&handler, &JsValue::undefined(), &[event_obj.clone().into()]);
                                }
                            }
                            
                            // Update the readyState property
                            let ws_obj = ctx.global_object().get_property("WebSocket", ctx)?;
                            if let Ok(instances) = ws_obj.get_property("instances", ctx) {
                                if let Ok(instance) = instances.get_property(&connection.id().to_string(), ctx) {
                                    instance.set("readyState", 3, true, ctx)?;
                                }
                            }
                            
                            Ok(JsValue::undefined())
                        });
                        
                        // Schedule the task
                        async_runtime.schedule_task(task);
                    },
                    Err(e) => {
                        // Create a task to dispatch the error event
                        let connection_clone = connection_clone.clone();
                        let error_message = e.to_string();
                        
                        let task = Task::new(move |ctx| {
                            // Update the ready state
                            {
                                let mut connection = connection_clone.lock().unwrap();
                                connection.set_ready_state(3); // CLOSED
                            }
                            
                            // Get the event handlers
                            let connection = connection_clone.lock().unwrap();
                            let handlers = connection.get_event_handlers("error");
                            
                            // Create the event object
                            let event_obj = ctx.create_object();
                            event_obj.set("type", "error", true, ctx)?;
                            event_obj.set("message", error_message, true, ctx)?;
                            
                            // Call the event handlers
                            for handler in handlers {
                                if handler.is_function() {
                                    let _ = ctx.call(&handler, &JsValue::undefined(), &[event_obj.clone().into()]);
                                }
                            }
                            
                            // Update the readyState property
                            let ws_obj = ctx.global_object().get_property("WebSocket", ctx)?;
                            if let Ok(instances) = ws_obj.get_property("instances", ctx) {
                                if let Ok(instance) = instances.get_property(&connection.id().to_string(), ctx) {
                                    instance.set("readyState", 3, true, ctx)?;
                                }
                            }
                            
                            Ok(JsValue::undefined())
                        });
                        
                        // Schedule the task
                        async_runtime.schedule_task(task);
                    },
                }
            });
            
            Ok(JsValue::undefined())
        });
        
        // Schedule the task
        async_runtime.schedule_task(task);
        
        // Store the WebSocket instance
        let ws_constructor = context.global_object().get_property("WebSocket", context)?;
        if let Ok(instances) = ws_constructor.get_property("instances", context) {
            instances.set(&id.to_string(), ws_obj.clone(), true, context)?;
        }
        
        Ok(ws_obj.into())
    }
}

#[async_trait]
impl Extension for WebSocketModule {
    fn name(&self) -> &str {
        "websocket"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the WebSocket constructor
        let ws_constructor = {
            let websocket_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                websocket_module.create_websocket(args, ctx)
            }
        };
        
        // Create the WebSocket constructor object
        let ws_obj = ObjectInitializer::new(context)
            .constructor(ws_constructor)
            .build();
        
        // Add the ready state constants
        ws_obj.set("CONNECTING", 0, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        ws_obj.set("OPEN", 1, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        ws_obj.set("CLOSING", 2, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        ws_obj.set("CLOSED", 3, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        // Create an object to store WebSocket instances
        let instances = context.create_object();
        ws_obj.set("instances", instances, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        // Add the WebSocket constructor to the global object
        let global = context.global_object();
        global.set("WebSocket", ws_obj, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for WebSocketModule {
    fn clone(&self) -> Self {
        WebSocketModule {
            config: self.config.clone(),
            connections: self.connections.clone(),
            next_id: self.next_id.clone(),
            async_runtime: self.async_runtime.clone(),
        }
    }
}
