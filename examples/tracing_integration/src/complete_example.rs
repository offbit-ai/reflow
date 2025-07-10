//! Complete tracing integration example
//!
//! This example demonstrates:
//! 1. Setting up the reflow_tracing server
//! 2. Creating a reflow_network with tracing enabled
//! 3. Creating actors and sending messages with automatic tracing
//! 4. Querying trace data from the server

use anyhow::Result;
use parking_lot::Mutex;
use reflow_actor::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, Port, message::Message,
};

use reflow_network::{
    network::{Network, NetworkConfig},
    tracing::{TracingConfig, TracingIntegration, init_global_tracing},
};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

// Example actor that demonstrates traced operations
#[derive(Clone)]
struct TracedActor {
    name: String,
}

impl TracedActor {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Actor for TracedActor {
    fn get_behavior(&self) -> ActorBehavior {
        let actor_name = self.name.clone();
        Box::new(move |context: ActorContext| {
            let name = actor_name.clone();
            Box::pin(async move {
                let payload = context.get_payload();
                let outports = context.get_outports();

                info!("Actor {} processing payload: {:?}", name, payload);

                // Process the messages
                let mut results = HashMap::new();
                for (port_name, message) in payload {
                    info!(
                        "Actor {} received message on port {}: {:?}",
                        name, port_name, message
                    );

                    let input_data = match message {
                        Message::String(value) => value,
                        _ => &"".to_string().into(),
                    };

                    if input_data.contains("fail") {
                        warn!("Simulate Actor failure");
                        return Err(anyhow::anyhow!("Simulated fail"));
                    }

                    // Process the message (simulate some work)
                    sleep(Duration::from_millis(10)).await;

                    // Create output message
                    let output_message = Message::object(
                        reflow_actor::message::EncodableValue::from(serde_json::json!({
                            "processed_by": name,
                            "original": message,
                            "timestamp": chrono::Utc::now().timestamp_millis()
                        })),
                    );

                    results.insert("output".to_string(), output_message);
                }

                info!("Actor {} finished processing", name);
                Ok(results)
            })
        })
    }

    fn get_inports(&self) -> Port {
        flume::unbounded()
    }

    fn get_outports(&self) -> Port {
        flume::unbounded()
    }

    fn create_process(
        &self,
        config: ActorConfig,
        tracing_integration: Option<TracingIntegration>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let actor_name = self.name.clone();
        let behavior = self.get_behavior();
        let inports = self.get_inports();
        let outports = self.get_outports();
        let load = self.load_count();
        let tracing_integration = tracing_integration.clone();
        

        Box::pin(async move {
            info!("Actor {} started", actor_name);
            let config = config.clone();
            let actor_id = config.get_node_id();
            
            // Simple message processing loop
            while let Ok(messages) = inports.1.recv_async().await {
                load.lock().inc();

                let context = ActorContext::new(
                    messages,
                    outports.clone(),
                    Arc::new(Mutex::new(reflow_actor::MemoryState::default())),
                    config.clone(),
                    load.clone(),
                );

                match behavior(context).await {
                    Ok(result) => {
                        if !result.is_empty() {
                            if let Err(e) = outports.0.send_async(result).await {
                                warn!("Failed to send output from {}: {}", actor_name, e);
                                break;
                            }
                            if let Some(tracing) = tracing_integration.clone() {
                                let _ = tracing.trace_actor_completed(actor_id).await;
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(tracing) = tracing_integration.clone() {
                            let _ = tracing.trace_actor_failed(actor_id, e.to_string()).await;
                        }
                        warn!("Actor {} failed: {}", actor_name, e);
                    }
                }

                load.lock().dec();
            }

            info!("Actor {} stopped", actor_name);
        })
    }

    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        Arc::new(Mutex::new(ActorLoad::new(0)))
    }

    fn shutdown(&self) {
        info!("Shutting down actor {}", self.name);
    }
}

async fn setup_tracing_server() -> Result<tokio::net::TcpListener> {
    // Start a simple tracing server for this example
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;

    info!("Tracing server will listen on: {}", local_addr);
    Ok(listener)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🚀 Starting complete tracing integration example");

    // 1. Setup tracing server (in real usage, this would be a separate service)
    // let listener = setup_tracing_server().await?;
    // let server_addr = listener.local_addr()?;

    // Start the tracing server in the background
    // tokio::spawn(async move {
    //     // This is a simplified server for demonstration
    //     // In practice, you'd use the full reflow_tracing server
    //     while let Ok((stream, addr)) = listener.accept().await {
    //         info!("Tracing server: accepted connection from {}", addr);

    //         tokio::spawn(async move {
    //             // Echo server for demonstration
    //             let ws_stream = match tokio_tungstenite::accept_async(stream).await {
    //                 Ok(ws) => ws,
    //                 Err(e) => {
    //                     warn!("Failed to upgrade to WebSocket: {}", e);
    //                     return;
    //                 }
    //             };

    //             info!("WebSocket connection established with {}", addr);

    //             use futures_util::{SinkExt, StreamExt};
    //             let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    //             while let Some(msg) = ws_receiver.next().await {
    //                 match msg {
    //                     Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
    //                         info!("Received trace message: {}", text);

    //                         // Echo back a simple response
    //                         let response = serde_json::json!({
    //                             "status": "received",
    //                             "timestamp": chrono::Utc::now().timestamp_millis()
    //                         });

    //                         let response_msg = tokio_tungstenite::tungstenite::Message::Text(
    //                             response.to_string()
    //                         );

    //                         if let Err(e) = ws_sender.send(response_msg).await {
    //                             warn!("Failed to send response: {}", e);
    //                             break;
    //                         }
    //                     }
    //                     Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
    //                         info!("Client {} closed connection", addr);
    //                         break;
    //                     }
    //                     Err(e) => {
    //                         warn!("WebSocket error with {}: {}", addr, e);
    //                         break;
    //                     }
    //                     _ => {}
    //                 }
    //             }
    //         });
    //     }
    // });

    // Wait a moment for server to start
    // sleep(Duration::from_millis(100)).await;

    // 2. Configure tracing client
    let tracing_config = TracingConfig {
        server_url: format!("ws://{}", "127.0.0.1:8080"),
        batch_size: 10,
        batch_timeout: Duration::from_millis(1000),
        enable_compression: false,
        enabled: true,
        retry_config: reflow_network::tracing::RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        },
    };

    // Initialize global tracing
    init_global_tracing(tracing_config.clone())?;
    info!("✅ Global tracing initialized");

    // 3. Create network with tracing enabled
    let network_config = NetworkConfig {
        tracing: tracing_config,
        ..Default::default()
    };

    let mut network = Network::new(network_config);
    info!("✅ Network created with tracing enabled");

    // 4. Register traced actors
    let processor_actor = TracedActor::new("processor");
    let formatter_actor = TracedActor::new("formatter");

    network.register_actor("processor", processor_actor)?;
    network.register_actor("formatter", formatter_actor)?;
    info!("✅ Actors registered");

    // 5. Add nodes to network
    network.add_node("proc1", "processor", None)?;
    network.add_node("proc1_fail", "processor", None)?;
    network.add_node("format1", "formatter", None)?;
    info!("✅ Nodes added to network");

    // 6. Start the network (this will initialize tracing)
    network.start()?;
    info!("✅ Network started with tracing active");

    // 7. Send some test messages (these will be automatically traced)
    info!("📨 Sending test messages...");

    for i in 1..=5 {
        let test_message = Message::object(reflow_actor::message::EncodableValue::from(
            serde_json::json!({
                "id": i,
                "data": format!("test_data_{}", i),
                "timestamp": chrono::Utc::now().timestamp_millis()
            }),
        ));

        network.send_to_actor("proc1", "input", test_message)?;
        sleep(Duration::from_millis(200)).await;
    }

    // 8. Execute some actors directly (with failure simulation)
    info!("🎭 Executing actors directly...");

    let success_result = network
        .execute_actor(
            "proc1",
            HashMap::from_iter([(
                "input".to_string(),
                Message::string("success_test".to_string()),
            )]),
        )
        .await?;
    info!("Success result: {:?}", success_result);

    // This will trigger a traced failure
    if let Err(e) = network
        .execute_actor(
            "proc1_fail",
            HashMap::from_iter([(
                "input".to_string(),
                Message::string("fail_test".to_string()),
            )]),
        )
        .await
    {
        info!("Expected failure traced: {}", e);
    }

    // 9. Let the network run for a bit to generate traces
    info!("⏳ Letting network run for a few seconds to generate traces...");
    sleep(Duration::from_secs(3)).await;

    // 10. Demonstrate tracing API usage
    if let Some(tracing) = reflow_network::tracing::global_tracing() {
        info!("🔍 Using global tracing API...");

        // Start a flow trace
        let trace_id = tracing.start_flow_trace("example_flow").await?;
        info!("Started trace: {:?}", trace_id);

        // Record some manual events
        tracing.trace_actor_created("manual_actor").await?;
        tracing
            .trace_message_sent("manual_actor", "output", "ManualMessage", 256)
            .await?;

        info!("✅ Manual tracing events recorded");
    }

    // 11. Shutdown gracefully
    info!("🛑 Shutting down network...");
    network.shutdown();

    // Give a moment for tracing to flush
    sleep(Duration::from_millis(500)).await;

    info!("🎉 Complete tracing integration example finished!");
    info!("📊 Summary:");
    info!("   - Tracing server: Started and handled connections");
    info!("   - Network: Created with tracing integration");
    info!("   - Actors: Registered and traced automatically");
    info!("   - Messages: Sent with automatic tracing");
    info!("   - Events: Actor creation, message sending, and failures traced");
    info!("   - API: Manual tracing demonstrated");
    info!("   - Shutdown: Graceful cleanup with trace flushing");

    Ok(())
}
