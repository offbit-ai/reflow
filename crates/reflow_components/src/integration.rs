//! Integration Components
//!
//! This module contains components that facilitate interaction with external systems.

use std::{collections::HashMap, sync::Arc};

use actor_macro::actor;
use anyhow::Error;

use parking_lot::Mutex;
use reflow_network::message::EncodableValue;
use reqwest::header::HeaderName;
use serde::de::value;
use serde_json::json;

use crate::{Actor, ActorBehavior, ActorPayload, ActorState, MemoryState, Message, Network, Port};

/// Makes HTTP requests to external services.
///
/// # Inports
/// - `URL`: Target URL for the request
/// - `Method`: HTTP method (GET, POST, etc.)
/// - `Headers`: Optional HTTP headers
/// - `Body`: Optional request body
///
/// # Outports
/// - `Response`: HTTP response data
/// - `Status`: HTTP status code
/// - `Error`: Error information if request failed
#[actor(
    HttpRequestActor,
    inports::<100>(URL, Method, Headers, Body),
    outports::<50>(Response, Status, Error),
    state(MemoryState)
)]
async fn http_request_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    let url = match payload.get("URL") {
        Some(Message::String(u)) => u.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("URL is required".to_string()),
            )]
            .into())
        }
    };

    let method = match payload.get("Method") {
        Some(Message::String(m)) => m.to_uppercase(),
        _ => "GET".to_string(), // Default to GET
    };

    let headers = serde_json::from_value::<HashMap<String, String>>(serde_json::to_value(
        payload.get("Headers").unwrap_or(&Message::Object(
            json!(HashMap::<String, serde_json::Value>::new()).into(),
        )),
    )?)?;
    let body = payload
        .get("Body")
        .cloned()
        .unwrap_or(Message::String("".to_string()));

    // Use reqwest to make the HTTP request
    let client = reqwest::Client::new();
    let req = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
        &url,
    );

    let mut req_headers = reqwest::header::HeaderMap::new();

    for (k, v) in headers {
        let key = HeaderName::from_bytes(k.as_bytes())?;
        let value = reqwest::header::HeaderValue::from_str(&v)?;
        req_headers.insert(key, value);
    }

    let req = req.headers(req_headers);
    let req = req.body(json!(body).to_string());

    let req = req.build()?;

    let resp = client.execute(req).await?;
    let status_code = resp.status().as_u16();
    // Check if the request was successful
    if !resp.status().is_success() {
        return Ok([(
            "Error".to_owned(),
            Message::Error(format!("Request failed with status code: {}", status_code)),
        )]
        .into());
    }

    let body = resp.bytes().await?;

    let mut result = HashMap::new();

    result.insert("Response".to_owned(), Message::Encoded(body.to_vec()));
    result.insert("Status".to_owned(), Message::Integer(status_code as i64));
    Ok(result)
}

/// Makes HTTP requests to external streaming services.
///
/// This will asynchronously stream the response body to the outport.
///
///
/// # Inports
/// - `URL`: Target URL for the request
/// - `Method`: HTTP method (GET, POST, etc.)
/// - `Headers`: Optional HTTP headers
/// - `Body`: Optional request body
///
/// # Outports
/// - `Response`: HTTP response data
/// - `Status`: HTTP status code
/// - `Error`: Error information if request failed
#[actor(
    HttpStreamActor,
    inports::<100>(URL, Method, Headers, Body),
    outports(Response, Done, Status, Error),
    state(MemoryState)
)]
async fn http_stream_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    let url = match payload.get("URL") {
        Some(Message::String(u)) => u.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("URL is required".to_string()),
            )]
            .into())
        }
    };

    let method = match payload.get("Method") {
        Some(Message::String(m)) => m.to_uppercase(),
        _ => "GET".to_string(), // Default to GET
    };

    let headers = serde_json::from_value::<HashMap<String, String>>(serde_json::to_value(
        payload.get("Headers").unwrap_or(&Message::Object(
            json!(HashMap::<String, serde_json::Value>::new()).into(),
        )),
    )?)?;
    let body = payload
        .get("Body")
        .cloned()
        .unwrap_or(Message::String("".to_string()));

    // Use reqwest to make the HTTP request
    let client = reqwest::Client::new();
    let req = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
        &url,
    );

    let mut req_headers = reqwest::header::HeaderMap::new();

    for (k, v) in headers {
        let key = HeaderName::from_bytes(k.as_bytes())?;
        let value = reqwest::header::HeaderValue::from_str(&v)?;
        req_headers.insert(key, value);
    }

    let req = req.headers(req_headers);
    let req = req.body(json!(body).to_string());

    let req = req.build()?;

    let mut resp = client.execute(req).await?;
    let status_code = resp.status().as_u16();
    // Check if the request was successful
    if !resp.status().is_success() {
        return Ok([(
            "Error".to_owned(),
            Message::Error(format!("Request failed with status code: {}", status_code)),
        )]
        .into());
    }

    while let Some(chunk) = resp.chunk().await? {
        let res = outport_channels
            .0
            .send_async(HashMap::from([(
                "Response".to_string(),
                Message::Stream(chunk.to_vec()),
            )]))
            .await;
        if res.is_err() {
            outport_channels
                .0
                .send_async(HashMap::from([(
                    "Error".to_string(),
                    Message::Error(res.err().unwrap().to_string()),
                )]))
                .await?;
            break;
        }
    }
    outport_channels
        .0
        .send_async(HashMap::from([(
            "Done".to_string(),
            Message::Boolean(true),
        )]))
        .await?;

    let mut result = HashMap::new();
    result.insert("Status".to_owned(), Message::Integer(status_code as i64));

    Ok(result)
}

/// Reads from or writes to a file.
///
/// # Inports
/// - `Path`: File path
/// - `Operation`: Read or write
/// - `Content`: Data to write (for write operations)
///
/// # Outports
/// - `Data`: File contents (for read operations)
/// - `Success`: Boolean indicating operation success
/// - `Error`: Error information if operation failed
#[actor(
    FileIOActor,
    inports::<100>(Path, Operation, Content),
    outports::<50>(Data, Success, Error),
    state(MemoryState)
)]
async fn file_io_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    use std::fs;
    let path = match payload.get("Path") {
        Some(Message::String(p)) => p.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("Path is required".to_string()),
            )]
            .into())
        }
    };

    let operation = match payload.get("Operation") {
        Some(Message::String(o)) => o.to_lowercase(),
        _ => "read".to_string(), // Default to read
    };

    let mut result = HashMap::new();
    let exists = fs::exists(&path)?;
    let is_directory = fs::metadata(&path)?.is_dir();

    match operation.as_str() {
        "read" => {
            if !exists || is_directory {
                result.insert(
                    "Error".to_owned(),
                    Message::Error("File not found".to_string()),
                );
                result.insert("Success".to_owned(), Message::Boolean(false));
                return Ok(result);
            }
            let data = fs::read(&path)?;

            result.insert("Data".to_owned(), Message::Encoded(data.to_vec()));
            result.insert("Success".to_owned(), Message::Boolean(true));
        }
        "write" => {
            let content = payload
                .get("Content")
                .cloned()
                .unwrap_or(Message::String("".to_string()));

            match content {
                Message::String(content) => {
                    fs::write(&path, content)?;
                }
                Message::Encoded(content) => {
                    fs::write(&path, content)?;
                }
                _ => {
                    result.insert(
                        "Error".to_owned(),
                        Message::Error("Invalid content type".to_string()),
                    );
                    result.insert("Success".to_owned(), Message::Boolean(false));
                    return Ok(result);
                }
            }

            result.insert("Success".to_owned(), Message::Boolean(true));
        }
        _ => {
            result.insert(
                "Error".to_owned(),
                Message::Error(format!("Unknown operation: {}", operation)),
            );
            result.insert("Success".to_owned(), Message::Boolean(false));
        }
    }

    Ok(result)
}

/// Executes a NuShell.
///
/// # Inports
/// - `Command`: Command to execute
/// - `WorkingDir`: Working directory
///
/// # Outports
/// - `Result`: Command standard output
/// - `Error`: Error information if execution failed
#[actor(
    NuShellActor,
    inports::<100>(Command),
    outports::<50>(Result, Error),
    state(MemoryState)
)]
async fn nushell_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    
    use nu_engine::eval_block;
    use nu_parser::parse;
    use nu_protocol::{
        engine::{EngineState, Stack, StateWorkingSet},
        PipelineData, Span, Value,
        debugger:: WithoutDebug
    };
    use anyhow::Result;
    use std::path::PathBuf;

    let mut engine_state = EngineState::new();
    let mut working_set = StateWorkingSet::new(&engine_state);
    let mut stack = Stack::new();

    let command = match payload.get("Command") {
        Some(Message::String(c)) => c.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("Command is required".to_string()),
            )]
            .into())
        }
    };
    let working_dir = match payload.get("WorkingDir") {
        Some(Message::String(w)) => w.clone(),
        _ => ".".to_string(), // Default to current directory
    };
    let cwd = PathBuf::from(working_dir);
    let block = parse(&mut working_set, None, command.as_bytes(), false);

    let mut final_result = HashMap::new();

    fn value_to_message(engine_state: &mut EngineState, value: &Value) -> Result<Message> {
        match value {
            Value::List { vals, .. } => {
                let mut array = vec![];
                for v in vals {
                    array.push(json!(v.as_str()?).into());
                }
                Ok(Message::Array(array))
            }
            Value::Bool { val, internal_span } => Ok(Message::Boolean((*val).into())),
            Value::Int { val, internal_span } => Ok(Message::Integer(*val)),
            Value::Float { val, internal_span } => Ok(Message::Float(*val)),
            Value::String { val, internal_span } => Ok(Message::String(val.to_string())),
            Value::Glob {
                val,
                no_expand,
                internal_span,
            } => todo!(),
            Value::Filesize { val, internal_span } => Ok(Message::Integer(val.get())),
            Value::Duration { val, internal_span } => Ok(Message::Integer(*val)),
            Value::Date { val, internal_span } => Ok(Message::String(val.to_string())),
            Value::Range { val, internal_span } => match &**val {
                nu_protocol::Range::IntRange(int_range) => {
                    let range_object = serde_json::to_value(int_range)?;
                    Ok(Message::Object(range_object.into()))
                }
                nu_protocol::Range::FloatRange(float_range) => {
                    let range_object = serde_json::to_value(float_range)?;
                    Ok(Message::Object(range_object.into()))
                }
            },
            Value::Record { val, internal_span } => {
                let mut record = HashMap::new();
                for (k, v) in val.iter() {
                    record.insert(k.to_string(), serde_json::to_value(value_to_message(engine_state, v)?)?);
                }
                Ok(Message::Object(json!(record).into()))
            }
            Value::Closure { val, internal_span } => {
                Ok(Message::String(val.coerce_into_string(&engine_state, internal_span.clone())?.into()))
            }
            Value::Error {
                error,
                internal_span,
            } => Ok(Message::Error(error.to_string())),
            Value::Binary { val, internal_span } => Ok(Message::Encoded(val.clone())),
            Value::CellPath { val, internal_span } => Ok(Message::String(val.to_string())),
            Value::Custom { val, internal_span } => todo!(),
            Value::Nothing { internal_span } => Ok(Message::Optional(None)),
        }
    }

  
    match eval_block::<WithoutDebug>(
        &mut engine_state,
        &mut stack,
        &block,
        PipelineData::Empty,
    ) {
        Ok(result) => {
            let value = result.into_value(Span::new(0, 0))?;
            let msg = value_to_message(&mut engine_state, &value)?;

            final_result.insert("Result".to_owned(), msg);

        }
        Err(e) =>  {
            final_result.insert(
                "Error".to_owned(),
                Message::Error(format!("Failed to execute command: {}", e.to_string())),
            );
        }
    }

    Ok(final_result)
}

/// Connects to a database.
///
/// # Inports
/// - `ConnectionString`: Database connection string
/// - `Query`: SQL query to execute
/// - `Params`: Query parameters
///
/// # Outports
/// - `Results`: Query results
/// - `Error`: Error information if query failed
#[actor(
    DatabaseActor,
    inports::<100>(ConnectionString, Query, Params),
    outports::<50>(Results, Error),
    state(MemoryState)
)]
async fn database_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    let connection_string = match payload.get("ConnectionString") {
        Some(Message::String(c)) => c.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("Connection string is required".to_string()),
            )]
            .into())
        }
    };

    let query = match payload.get("Query") {
        Some(Message::String(q)) => q.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("Query is required".to_string()),
            )]
            .into())
        }
    };

    // In a real implementation, this would execute a database query
    // For now, just simulate responses
    let mut result = HashMap::new();

    // Simulate query execution
    if query.to_lowercase().starts_with("select") {
        // Create a sample result set
        let mut rows = Vec::new();

        let row1 = serde_json::json!({
            "id": 1,
            "name": "Sample Item 1",
            "value": 42
        });

        let row2 = serde_json::json!({
            "id": 2,
            "name": "Sample Item 2",
            "value": 84
        });

        rows.push(row1.into());
        rows.push(row2.into());

        result.insert("Results".to_owned(), Message::Array(rows));
    } else if query.to_lowercase().starts_with("insert")
        || query.to_lowercase().starts_with("update")
        || query.to_lowercase().starts_with("delete")
    {
        // Simulate a write operation
        let affected = serde_json::json!({
            "affectedRows": 1,
            "success": true
        });

        result.insert("Results".to_owned(), Message::Object(affected.into()));
    } else {
        result.insert(
            "Error".to_owned(),
            Message::Error("Invalid SQL query".to_string()),
        );
    }

    Ok(result)
}

/// Publishes or subscribes to a message queue.
///
/// # Inports
/// - `Topic`: Message topic/channel
/// - `Message`: Message to publish
/// - `Action`: Publish or subscribe
/// - `SubscriptionId`: Optional identifier for subscription (defaults to random UUID)
/// - `Unsubscribe`: Boolean to unsubscribe from a topic (requires SubscriptionId)
///
/// # Outports
/// - `Received`: Messages received from subscription
/// - `Success`: Boolean indicating operation success
/// - `Error`: Error information if operation failed
#[actor(
    MessageQueueActor,
    inports::<100>(Topic, Message, Action, SubscriptionId, Unsubscribe),
    outports::<50>(Received, Success, SubscriptionId, Error),
    state(MemoryState)
)]
async fn message_queue_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    outport_channels: Port,
) -> Result<HashMap<String, Message>, Error> {
    use uuid::Uuid;
    use tokio::sync::mpsc;
    use std::time::Duration;
    
    let topic = match payload.get("Topic") {
        Some(Message::String(t)) => t.clone(),
        _ => {
            return Ok([(
                "Error".to_owned(),
                Message::Error("Topic is required".to_string()),
            )]
            .into())
        }
    };

    let action = match payload.get("Action") {
        Some(Message::String(a)) => a.to_lowercase(),
        _ => "publish".to_string(), // Default to publish
    };
    
    // Get or generate subscription ID
    let subscription_id = match payload.get("SubscriptionId") {
        Some(Message::String(id)) => id.clone(),
        _ => Uuid::new_v4().to_string(),
    };
    
    // Check if unsubscribing
    let unsubscribe = match payload.get("Unsubscribe") {
        Some(Message::Boolean(true)) => true,
        _ => false,
    };

    let mut result = HashMap::new();

    // Initialize state structure if needed
    {
        let mut state_lock = state.lock();
        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {

            // Initialize topics if not exists
            if !state_data.has_key("topics") {
                state_data.insert("topics", serde_json::Value::Object(serde_json::Map::new()));
            }
            
            // Initialize subscriptions if not exists
            if !state_data.has_key("subscriptions") {
                state_data.insert("subscriptions", serde_json::Value::Object(serde_json::Map::new()));
            }
            
            // Initialize message queues if not exists
            if !state_data.has_key("message_queues") {
                state_data.insert("message_queues", serde_json::Value::Object(serde_json::Map::new()));
            }
        }
    }

    match action.as_str() {
        "publish" => {
            let message = payload
                .get("Message")
                .cloned()
                .unwrap_or(Message::String("".to_string()));

            // Store the message in state
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                // Get existing messages for this topic
                let topics = state_data
                    .get("topics")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                let mut new_topics = topics;

                // Add message to topic with timestamp
                let messages = new_topics
                    .get(&topic)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut new_messages = messages;
                let message_with_metadata = serde_json::json!({
                    "content": serde_json::to_value(&message).unwrap(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "id": Uuid::new_v4().to_string()
                });
                new_messages.push(message_with_metadata.clone());

                new_topics.insert(topic.clone(), serde_json::Value::Array(new_messages));
                state_data.insert("topics", serde_json::Value::Object(new_topics));
                
                // Get active subscriptions for this topic
                if let Some(subscriptions) = state_data.clone()
                    .get("subscriptions")
                    .and_then(|v| v.as_object())
                {
                    // Find subscribers for this topic
                    for (sub_id, sub_info) in subscriptions.iter() {
                        if let Some(sub_topic) = sub_info
                            .as_object()
                            .and_then(|o| o.get("topic"))
                            .and_then(|t| t.as_str())
                        {
                            if sub_topic == topic {
                                // Add message to subscriber's queue
                                let message_queues = state_data
                                    .get("message_queues")
                                    .and_then(|v| v.as_object())
                                    .cloned()
                                    .unwrap_or_default();
                                
                                let mut new_message_queues = message_queues;
                                
                                // Get or create queue for this subscription
                                let queue = new_message_queues
                                    .get(sub_id)
                                    .and_then(|v| v.as_array())
                                    .cloned()
                                    .unwrap_or_default();
                                
                                let mut new_queue = queue;
                                new_queue.push(message_with_metadata.clone());
                                
                                new_message_queues.insert(sub_id.clone(), serde_json::Value::Array(new_queue));
                                state_data.insert("message_queues", serde_json::Value::Object(new_message_queues));
                            }
                        }
                    }
                }
            }

            result.insert("Success".to_owned(), Message::Boolean(true));
        }
        "subscribe" => {
            // Handle unsubscribe request
            if unsubscribe {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    // Remove subscription
                    if let Some(subscriptions) = state_data
                        .get("subscriptions")
                        .and_then(|v| v.as_object())
                        .cloned()
                    {
                        let mut new_subscriptions = subscriptions;
                        new_subscriptions.remove(&subscription_id);
                        state_data.insert("subscriptions", serde_json::Value::Object(new_subscriptions));
                    }
                    
                    // Remove message queue
                    if let Some(message_queues) = state_data
                        .get("message_queues")
                        .and_then(|v| v.as_object())
                        .cloned()
                    {
                        let mut new_message_queues = message_queues;
                        new_message_queues.remove(&subscription_id);
                        state_data.insert("message_queues", serde_json::Value::Object(new_message_queues));
                    }
                }
                
                result.insert("Success".to_owned(), Message::Boolean(true));
                result.insert("SubscriptionId".to_owned(), Message::String(subscription_id));
                return Ok(result);
            }
            
            // Register new subscription
            {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    // Add subscription
                    let subscriptions = state_data
                        .get("subscriptions")
                        .and_then(|v| v.as_object())
                        .cloned()
                        .unwrap_or_default();
                    
                    let mut new_subscriptions = subscriptions;
                    
                    // Create subscription entry
                    let subscription = serde_json::json!({
                        "topic": topic.clone(),
                        "created": chrono::Utc::now().to_rfc3339(),
                        "last_active": chrono::Utc::now().to_rfc3339(),
                        "status": "active"
                    });
                    
                    new_subscriptions.insert(subscription_id.clone(), subscription);
                    state_data.insert("subscriptions", serde_json::Value::Object(new_subscriptions));
                    
                    // Initialize message queue for this subscription
                    let message_queues = state_data
                        .get("message_queues")
                        .and_then(|v| v.as_object())
                        .cloned()
                        .unwrap_or_default();
                    
                    let mut new_message_queues = message_queues;
                    new_message_queues.insert(subscription_id.clone(), serde_json::Value::Array(Vec::new()));
                    state_data.insert("message_queues", serde_json::Value::Object(new_message_queues));
                    
                    // Return any existing messages for this topic
                    let messages = state_data
                        .get("topics")
                        .and_then(|v| v.as_object())
                        .and_then(|topics| topics.get(&topic))
                        .and_then(|v| v.as_array())
                        .map(|msgs| {
                            msgs.iter()
                                .map(|msg| {
                                    msg.as_object()
                                        .and_then(|o| o.get("content"))
                                        .cloned()
                                        .unwrap_or(serde_json::Value::Null)
                                })
                                .filter(|v| !v.is_null())
                                .map(|v| Into::<EncodableValue>::into(v))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    
                    if !messages.is_empty() {
                        result.insert("Received".to_owned(), Message::Array(messages));
                    }
                }
            }
            
            // Start a background task to poll for new messages
            let state_clone = state.clone();
            let sub_id = subscription_id.clone();
            let outport_clone = outport_channels.clone();
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(100));
                let mut active = true;
                
                while active {
                    interval.tick().await;
                    
                    // Check if subscription is still active and get any new messages
                    let mut messages_to_send = Vec::new();
                    {
                        let mut state_lock = state_clone.lock();
                        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                            // Check if subscription still exists
                            let subscription_exists = state_data
                                .get("subscriptions")
                                .and_then(|v| v.as_object())
                                .map(|subs| subs.contains_key(&sub_id))
                                .unwrap_or(false);
                            
                            if !subscription_exists {
                                active = false;
                                continue;
                            }
                            
                            // Update last active timestamp
                            if let Some(subscriptions) = state_data
                                .get("subscriptions")
                                .and_then(|v| v.as_object())
                                .cloned()
                            {
                                if let Some(sub_info) = subscriptions.get(&sub_id).and_then(|v| v.as_object()) {
                                    let mut new_sub_info = sub_info.clone();
                                    new_sub_info.insert("last_active".to_string(), 
                                                       serde_json::Value::String(chrono::Utc::now().to_rfc3339()));
                                    
                                    let mut new_subscriptions = subscriptions.clone();
                                    new_subscriptions.insert(sub_id.clone(), serde_json::Value::Object(new_sub_info));
                                    state_data.insert("subscriptions", serde_json::Value::Object(new_subscriptions));
                                }
                            }
                            
                            // Get messages from queue
                            if let Some(message_queues) = state_data
                                .get("message_queues")
                                .and_then(|v| v.as_object())
                                .cloned()
                            {
                                if let Some(queue) = message_queues.get(&sub_id).and_then(|v| v.as_array()) {
                                    if !queue.is_empty() {
                                        // Extract message content from queue items
                                        messages_to_send = queue.iter()
                                            .map(|msg| {
                                                msg.as_object()
                                                    .and_then(|o| o.get("content"))
                                                    .cloned()
                                                    .unwrap_or(serde_json::Value::Null)
                                            })
                                            .filter(|v| !v.is_null())
                                            .map(|v| Into::<EncodableValue>::into(v))
                                            .collect();
                                        
                                        // Clear the queue
                                        let mut new_message_queues = message_queues;
                                        new_message_queues.insert(sub_id.clone(), serde_json::Value::Array(Vec::new()));
                                        state_data.insert("message_queues", serde_json::Value::Object(new_message_queues));
                                    }
                                }
                            }
                        }
                    }
                    
                    // Send messages if any
                    if !messages_to_send.is_empty() {
                        let res = outport_clone
                            .0
                            .send_async(HashMap::from([(
                                "Received".to_string(),
                                Message::Array(messages_to_send),
                            )]))
                            .await;
                        
                        if res.is_err() {
                            // If sending fails, subscription is likely closed
                            active = false;
                            
                            // Clean up subscription
                            let mut state_lock = state_clone.lock();
                            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                                // Remove subscription
                                if let Some(subscriptions) = state_data
                                    .get("subscriptions")
                                    .and_then(|v| v.as_object())
                                    .cloned()
                                {
                                    let mut new_subscriptions = subscriptions;
                                    new_subscriptions.remove(&sub_id);
                                    state_data.insert("subscriptions", serde_json::Value::Object(new_subscriptions));
                                }
                                
                                // Remove message queue
                                if let Some(message_queues) = state_data
                                    .get("message_queues")
                                    .and_then(|v| v.as_object())
                                    .cloned()
                                {
                                    let mut new_message_queues = message_queues;
                                    new_message_queues.remove(&sub_id);
                                    state_data.insert("message_queues", serde_json::Value::Object(new_message_queues));
                                }
                            }
                        }
                    }
                }
            });

            result.insert("Success".to_owned(), Message::Boolean(true));
            result.insert("SubscriptionId".to_owned(), Message::String(subscription_id));
        }
        _ => {
            result.insert(
                "Error".to_owned(),
                Message::Error(format!("Unknown action: {}", action)),
            );
            result.insert("Success".to_owned(), Message::Boolean(false));
        }
    }

    Ok(result)
}
