//! Custom actors for the multi-graph workspace example
//! 
//! These are simple, tested actors that work specifically for this demo.

use std::{collections::HashMap, sync::Arc};
use actor_macro::actor;
use anyhow::Error;
use reflow_network::{
    actor::{Actor, ActorConfig, ActorBehavior, ActorContext, ActorLoad, MemoryState, Port},
    message::Message,
    message::EncodableValue
};

/// Simple timer actor that emits periodic events
/// 
/// # Inports
/// - `Start`: Boolean to start the timer
/// 
/// # Outports  
/// - `Output`: Emitted on each timer tick
#[actor(
    SimpleTimerActor,
    inports::<100>(Start),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn simple_timer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();

    let interval_secs = context.get_config().get_number("interval").unwrap_or(1000.0);

    // Check if we should start the timer
    if let Some(start_msg) = payload.get("Start") {
        let should_start = match start_msg {
            Message::Boolean(b) => *b,
            Message::Integer(i) => *i != 0,
            Message::String(s) => !s.is_empty(),
            _ => true,
        };

        if should_start {
            // Store timer state
            {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    state_data.insert("running", serde_json::json!(true));
                    state_data.insert("interval", serde_json::json!(interval_secs));
                    state_data.insert("tick_count", serde_json::json!(0));
                }
            }

            // Get max ticks (default to 10 for demos to prevent infinite loops)
            let max_ticks = payload
                .get("MaxTicks")
                .and_then(|m| match m {
                    Message::Integer(i) => Some(*i as u64),
                    Message::Float(f) => Some(*f as u64),
                    _ => None,
                })
                .unwrap_or(10); // Default to 10 ticks for demos

            // Spawn timer task with proper load management
            let state_clone = state.clone();
            let outports = outport_channels.clone();
            let load = context.get_load();
            
            // Increase load count for the background task
            load.lock().inc();
            
            tokio::spawn(async move {
                let mut tick_count = 0;
                
                // Ensure we decrease load count when the task finishes
                let _load_guard = scopeguard::guard(load.clone(), |load| {
                    load.lock().dec();
                });
                
                loop {
                    // Check if timer should still be running and hasn't exceeded max ticks
                    let should_continue = {
                        let state_lock = state_clone.lock();
                        if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                            let running = state_data
                                .get("running")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let current_ticks = state_data
                                .get("tick_count")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as u64;
                            running && current_ticks < max_ticks
                        } else {
                            false
                        }
                    };

                    if !should_continue {
                        // Stop the timer and mark as not running
                        {
                            let mut state_lock = state_clone.lock();
                            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                                state_data.insert("running", serde_json::json!(false));
                            }
                        }
                        println!("🔴 SimpleTimerActor stopped after {} ticks", tick_count);
                        break;
                    }

                    // Wait for intervalr
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs as u64)).await;

                    // Increment tick count
                    tick_count += 1;
                    
                    // Update state
                    {
                        let mut state_lock = state_clone.lock();
                        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                            state_data.insert("tick_count", serde_json::json!(tick_count));
                        }
                    }

                    // Send tick message
                    let tick_message = Message::object(
                        EncodableValue::from(serde_json::json!({
                            "tick": tick_count,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "source": "SimpleTimerActor",
                            "max_ticks": max_ticks
                        }))
                    );

                    if outports.0.send_async(HashMap::from([
                        ("Output".to_owned(), tick_message)
                    ])).await.is_err() {
                        // Channel closed, stop the timer
                        break;
                    }
                }
            });

            println!("✅ SimpleTimerActor started with interval: {}s", interval_secs);
        }
    }

    Ok(HashMap::new())
}

/// Simple logger actor that logs incoming messages
/// 
/// # Inports
/// - `Input`: Input data to log
/// - `Prefix`: Optional prefix for log messages
/// 
/// # Outports
/// - `Output`: Passes through the input data
#[actor(
    SimpleLoggerActor,
    inports::<100>(Input, Prefix),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn simple_logger_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();

    if let Some(input_msg) = payload.get("Input") {
        // Get prefix from payload or state
        let prefix = if let Some(Message::String(p)) = payload.get("Prefix") {
            p.clone()
        } else {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("prefix")
                    .and_then(|v| v.as_str())
                    .unwrap_or("LOG")
                    .to_string().into()
            } else {
                "LOG".to_string().into()
            }
        };

        // Log the message with timestamp
        let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f");
        println!("[{}] {}: {:?}", timestamp, prefix, input_msg);

        // Update log count in state
        {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let count = state_data
                    .get("log_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) + 1;
                state_data.insert("log_count", serde_json::json!(count));
            }
        }

        // Pass through the input
        Ok(HashMap::from([("Output".to_owned(), input_msg.clone())]))
    } else {
        Ok(HashMap::new())
    }
}

/// Data generator actor that creates sample data
/// 
/// # Inports
/// - `Trigger`: Signal to generate data
/// - `Type`: Type of data to generate (number, string, object)
/// 
/// # Outports
/// - `Output`: Generated data
#[actor(
    DataGeneratorActor,
    inports::<100>(Trigger, Type),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn data_generator_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();

    if payload.contains_key("Trigger") {
        // Get data type from payload or state
        let data_type = if let Some(Message::String(t)) = payload.get("Type") {
            t.clone()
        } else {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("data_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("number")
                    .to_string().into()
            } else {
                "number".to_string().into()
            }
        };

        // Update generation count
        let generation_count = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let count = state_data
                    .get("generation_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) + 1;
                state_data.insert("generation_count", serde_json::json!(count));
                count
            } else {
                1
            }
        };

        // Generate data based on type
        let generated_data = match data_type.as_str() {
            "number" => Message::Integer(generation_count),
            "string" => Message::String(format!("generated_data_{}", generation_count).into()),
            "object" => Message::object(
                EncodableValue::from(serde_json::json!({
                    "id": generation_count,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "type": "generated",
                    "value": format!("sample_value_{}", generation_count)
                }))
            ),
            _ => Message::String(format!("unknown_type_data_{}", generation_count).into()),
        };

        Ok(HashMap::from([("Output".to_owned(), generated_data)]))
    } else {
        Ok(HashMap::new())
    }
}
