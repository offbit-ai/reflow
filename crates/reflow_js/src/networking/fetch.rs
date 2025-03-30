use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer, property::Attribute};
use crate::runtime::{Extension, ExtensionError};
use crate::security::PermissionManager;
use crate::async_runtime::{AsyncRuntime, Task};
use crate::networking::NetworkConfig;
use crate::networking::is_host_allowed;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Duration;
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, header::{HeaderMap, HeaderName, HeaderValue}};
use url::Url;
use tokio::sync::Semaphore;

/// Fetch module
#[derive(Debug)]
pub struct FetchModule {
    /// Network configuration
    config: NetworkConfig,
    
    /// HTTP client
    client: Client,
    
    /// Request semaphore
    semaphore: Arc<Semaphore>,
    
    /// Async runtime
    async_runtime: Option<Arc<AsyncRuntime>>,
}

impl FetchModule {
    /// Create a new fetch module
    pub fn new(config: NetworkConfig) -> Self {
        // Create the HTTP client
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout))
            .build()
            .unwrap_or_default();
        
        // Create the semaphore
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));
        
        FetchModule {
            config,
            client,
            semaphore,
            async_runtime: None,
        }
    }
    
    /// Set the async runtime
    pub fn set_async_runtime(&mut self, async_runtime: Arc<AsyncRuntime>) {
        self.async_runtime = Some(async_runtime);
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
    
    /// Create a request
    async fn create_request(&self, url: &str, options: Option<JsValue>, context: &mut Context) -> Result<RequestBuilder, JsError> {
        // Parse the URL
        let url = Url::parse(url).map_err(|e| JsError::from_opaque(format!("Invalid URL: {}", e).into()))?;
        
        // Check if the URL is allowed
        self.check_url(&url)?;
        
        // Create the request builder
        let mut builder = self.client.request(Method::GET, url);
        
        // Add default headers
        for (name, value) in &self.config.default_headers {
            builder = builder.header(name, value);
        }
        
        // Parse options
        if let Some(options) = options {
            if !options.is_object() {
                return Err(JsError::from_opaque("Options must be an object".into()));
            }
            
            // Get the method
            if let Ok(method) = options.get_property("method", context) {
                if !method.is_undefined() {
                    let method_str = method.to_string(context)?;
                    let method = match method_str.to_uppercase().as_str() {
                        "GET" => Method::GET,
                        "POST" => Method::POST,
                        "PUT" => Method::PUT,
                        "DELETE" => Method::DELETE,
                        "HEAD" => Method::HEAD,
                        "OPTIONS" => Method::OPTIONS,
                        "PATCH" => Method::PATCH,
                        _ => return Err(JsError::from_opaque(format!("Invalid method: {}", method_str).into())),
                    };
                    
                    builder = self.client.request(method, url);
                }
            }
            
            // Get the headers
            if let Ok(headers) = options.get_property("headers", context) {
                if !headers.is_undefined() && !headers.is_null() {
                    if headers.is_object() {
                        let headers_obj = headers.as_object().unwrap();
                        let keys = headers_obj.keys(context)?;
                        
                        for key in keys {
                            let value = headers_obj.get_property(&key, context)?;
                            let value_str = value.to_string(context)?;
                            
                            builder = builder.header(key, value_str);
                        }
                    } else {
                        return Err(JsError::from_opaque("Headers must be an object".into()));
                    }
                }
            }
            
            // Get the body
            if let Ok(body) = options.get_property("body", context) {
                if !body.is_undefined() && !body.is_null() {
                    let body_str = body.to_string(context)?;
                    builder = builder.body(body_str);
                }
            }
            
            // Get the timeout
            if let Ok(timeout) = options.get_property("timeout", context) {
                if !timeout.is_undefined() {
                    let timeout_ms = timeout.to_number(context)?;
                    builder = builder.timeout(Duration::from_millis(timeout_ms as u64));
                }
            }
        }
        
        Ok(builder)
    }
    
    /// Create a response object
    async fn create_response(&self, response: Response, context: &mut Context) -> JsResult<JsValue> {
        // Create the response object
        let response_obj = context.create_object();
        
        // Set the status
        response_obj.set("status", response.status().as_u16(), true, context)?;
        
        // Set the status text
        response_obj.set("statusText", response.status().canonical_reason().unwrap_or(""), true, context)?;
        
        // Set the URL
        response_obj.set("url", response.url().to_string(), true, context)?;
        
        // Set the headers
        let headers_obj = context.create_object();
        for (name, value) in response.headers() {
            headers_obj.set(name.as_str(), value.to_str().unwrap_or(""), true, context)?;
        }
        response_obj.set("headers", headers_obj, true, context)?;
        
        // Create the text method
        let response_clone = response.clone();
        let text_fn = {
            let async_runtime = self.async_runtime.clone();
            move |_this: &JsValue, _args: &[JsValue], ctx: &mut Context| {
                // Create a promise
                let async_runtime = match &async_runtime {
                    Some(runtime) => runtime.clone(),
                    None => return Err(JsError::from_opaque("Async runtime not available".into())),
                };
                
                let (promise, resolver) = async_runtime.create_promise();
                
                // Clone the response
                let response = response_clone.clone();
                
                // Create a task to get the text
                let task = Task::new(move |_ctx| {
                    // Resolve the promise with the text
                    let resolver_clone = resolver.clone();
                    tokio::spawn(async move {
                        match response.text().await {
                            Ok(text) => {
                                resolver_clone.resolve(move |_| Ok(JsValue::from(text)));
                            },
                            Err(e) => {
                                resolver_clone.reject(move |_| Ok(JsValue::from(e.to_string())));
                            },
                        }
                    });
                    
                    Ok(JsValue::undefined())
                });
                
                // Schedule the task
                async_runtime.schedule_task(task);
                
                // Return the promise
                Ok(JsValue::from(promise.id()))
            }
        };
        
        // Create the json method
        let response_clone = response;
        let json_fn = {
            let async_runtime = self.async_runtime.clone();
            move |_this: &JsValue, _args: &[JsValue], ctx: &mut Context| {
                // Create a promise
                let async_runtime = match &async_runtime {
                    Some(runtime) => runtime.clone(),
                    None => return Err(JsError::from_opaque("Async runtime not available".into())),
                };
                
                let (promise, resolver) = async_runtime.create_promise();
                
                // Clone the response
                let response = response_clone.clone();
                
                // Create a task to get the JSON
                let task = Task::new(move |ctx| {
                    // Resolve the promise with the JSON
                    let resolver_clone = resolver.clone();
                    tokio::spawn(async move {
                        match response.json::<serde_json::Value>().await {
                            Ok(json) => {
                                // Convert the JSON to a JsValue
                                let json_str = json.to_string();
                                resolver_clone.resolve(move |ctx| {
                                    let json_value = ctx.eval(json_str)?;
                                    Ok(json_value)
                                });
                            },
                            Err(e) => {
                                resolver_clone.reject(move |_| Ok(JsValue::from(e.to_string())));
                            },
                        }
                    });
                    
                    Ok(JsValue::undefined())
                });
                
                // Schedule the task
                async_runtime.schedule_task(task);
                
                // Return the promise
                Ok(JsValue::from(promise.id()))
            }
        };
        
        // Add the methods to the response object
        response_obj.set("text", text_fn, true, context)?;
        response_obj.set("json", json_fn, true, context)?;
        
        Ok(response_obj.into())
    }
    
    /// Fetch a URL
    fn fetch(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        // Check if we have at least one argument
        if args.is_empty() {
            return Err(JsError::from_opaque("fetch requires at least one argument".into()));
        }
        
        // Get the URL
        let url = args[0].to_string(context)?;
        
        // Get the options
        let options = if args.len() > 1 {
            Some(args[1].clone())
        } else {
            None
        };
        
        // Get the async runtime
        let async_runtime = match &self.async_runtime {
            Some(runtime) => runtime.clone(),
            None => return Err(JsError::from_opaque("Async runtime not available".into())),
        };
        
        // Create a promise
        let (promise, resolver) = async_runtime.create_promise();
        
        // Clone the necessary values
        let fetch_module = self.clone();
        let semaphore = self.semaphore.clone();
        
        // Create a task to perform the fetch
        let task = Task::new(move |ctx| {
            // Resolve the promise with the fetch result
            let resolver_clone = resolver.clone();
            tokio::spawn(async move {
                // Acquire a permit from the semaphore
                let _permit = match semaphore.acquire().await {
                    Ok(permit) => permit,
                    Err(e) => {
                        resolver_clone.reject(move |_| Ok(JsValue::from(format!("Failed to acquire semaphore: {}", e).into())));
                        return;
                    },
                };
                
                // Create the request
                let request = match fetch_module.create_request(&url, options, ctx).await {
                    Ok(request) => request,
                    Err(e) => {
                        resolver_clone.reject(move |_| Ok(JsValue::from(e.to_string())));
                        return;
                    },
                };
                
                // Send the request
                match request.send().await {
                    Ok(response) => {
                        // Create the response object
                        match fetch_module.create_response(response, ctx).await {
                            Ok(response_obj) => {
                                resolver_clone.resolve(move |_| Ok(response_obj));
                            },
                            Err(e) => {
                                resolver_clone.reject(move |_| Ok(JsValue::from(e.to_string())));
                            },
                        }
                    },
                    Err(e) => {
                        resolver_clone.reject(move |_| Ok(JsValue::from(e.to_string())));
                    },
                }
            });
            
            Ok(JsValue::undefined())
        });
        
        // Schedule the task
        async_runtime.schedule_task(task);
        
        // Return the promise
        Ok(JsValue::from(promise.id()))
    }
}

#[async_trait]
impl Extension for FetchModule {
    fn name(&self) -> &str {
        "fetch"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the fetch function
        let fetch_fn = {
            let fetch_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fetch_module.fetch(args, ctx)
            }
        };
        
        // Add the fetch function to the global object
        let global = context.global_object();
        global.set("fetch", fetch_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for FetchModule {
    fn clone(&self) -> Self {
        FetchModule {
            config: self.config.clone(),
            client: self.client.clone(),
            semaphore: self.semaphore.clone(),
            async_runtime: self.async_runtime.clone(),
        }
    }
}
