//! Simple Client Example - Basic Tracing Integration
//!
//! This example demonstrates:
//! - Creating simple actors that integrate with reflow_network tracing
//! - Sending trace events to reflow_tracing server
//! - Using the TracingIntegration high-level API
//! - Demonstrating different event types

use anyhow::Result;
use clap::Parser;
use colored::*;
use parking_lot::Mutex as ParkingLotMutex;
use reflow_actor::{Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, MemoryState, Port};
use reflow_network::tracing::{
    TracingClient, TracingConfig, TracingIntegration, init_global_tracing,
};
use reflow_network::network::{Network, NetworkConfig};
use reflow_tracing_protocol::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};

use reflow_actor::message::Message;
use reflow_network::connector::{InitialPacket, Connector, ConnectionPoint};

// Global tracing client for the simple client
static GLOBAL_CLIENT: std::sync::OnceLock<TracingClient> = std::sync::OnceLock::new();

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Tracing server URL
    #[arg(short, long, default_value = "ws://127.0.0.1:8080")]
    server_url: String,

    /// Number of messages to generate
    #[arg(short, long, default_value_t = 10)]
    count: usize,

    /// Delay between messages (milliseconds)
    #[arg(short, long, default_value_t = 1000)]
    delay: u64,

    /// Flow ID for tracing
    #[arg(short, long, default_value = "simple-example")]
    flow_id: String,

    /// Enable debug logging
    #[arg(short, long)]
    verbose: bool,
}

/// A simple data generator actor that sends messages
struct DataGenerator {
    message_count: usize,
    inports: Port,
    outports: Port,
    load: Arc<ParkingLotMutex<ActorLoad>>,
}

impl DataGenerator {
    fn new(message_count: usize) -> Self {
        Self {
            message_count,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ParkingLotMutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for DataGenerator {
    fn get_behavior(&self) -> ActorBehavior {
        let message_count = self.message_count;
        Box::new(move |context: ActorContext| {
            Box::pin(async move {
                info!(
                    "🚀 DataGenerator starting - will generate {} messages",
                    message_count
                );

                // // Send actor created event directly to the tracing server
                // if let Some(client) = get_global_client() {
                //     let event = TraceEvent::actor_created("data_generator".to_string());
                //     let trace_id = TraceId::new(); // In a real implementation, use the actual trace ID
                //     let _ = client.record_event(trace_id, event).await;
                // }

                let mut messages = HashMap::new();

                for i in 1..=message_count {
                    let message_data = format!("Generated message #{}", i);
                    info!("📤 Generated: {}", message_data.green());

                    // Create a message
                    messages.insert(
                        "output".to_string(),
                        reflow_actor::message::Message::string(message_data.clone()),
                    );

                    // // Send message sent event directly to tracing server
                    // if let Some(client) = get_global_client() {
                    //     let event = TraceEvent::message_sent(
                    //         "data_generator".to_string(),
                    //         "output".to_string(),
                    //         "String".to_string(),
                    //         message_data.len(),
                    //     );
                    //     let trace_id = TraceId::new();
                    //     let _ = client.record_event(trace_id, event).await;
                    // }

                    // Send via outports
                    if let Err(e) = context.get_outports().0.send(messages.clone()) {
                        error!("Failed to send message: {}", e);
                    }

                    // Wait between messages
                    time::sleep(Duration::from_millis(500)).await;
                }

                // Send actor completed event
                // if let Some(client) = get_global_client() {
                //     let event = TraceEvent::actor_completed("data_generator".to_string());
                //     let trace_id = TraceId::new();
                //     let _ = client.record_event(trace_id, event).await;
                // }

                Ok(HashMap::new())
            })
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn create_process(
        &self,
        config: ActorConfig,
        tracing_integration: Option<TracingIntegration>,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send + 'static>> {
        let behavior = self.get_behavior();
        let outports = self.get_outports();
        let state = Arc::new(ParkingLotMutex::new(MemoryState::default()));
        let load = self.load.clone();

        Box::pin(async move {
            let config = config.clone();
            let actor_id = config.get_node_id().clone();
            let context = ActorContext::new(
                HashMap::new(), // No input payload for generator
                outports,
                state,
                config.clone(),
                load,
            );

            if let Err(e) = behavior(context).await {
                error!("DataGenerator error: {}", e);
                if let Some(tracing) = tracing_integration.clone() {
                    let _ = tracing
                        .trace_actor_failed("data_generator", e.to_string())
                        .await;
                }
            }

            if let Some(tracing) = tracing_integration.clone() {
                let _ = tracing.trace_actor_completed("data_generator").await;
            }
        })
    }
}

/// A simple message processor that handles incoming data
struct MessageProcessor {
    inports: Port,
    outports: Port,
    load: Arc<ParkingLotMutex<ActorLoad>>,
}

impl MessageProcessor {
    fn new() -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ParkingLotMutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for MessageProcessor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(move |context: ActorContext| {
            Box::pin(async move {
                info!("🏃 MessageProcessor starting");

                // Trace actor creation
                // if let Some(tracing) = reflow_network::tracing::global_tracing() {
                //     let _ = tracing.trace_actor_created("message_processor").await;
                // }

                let mut processed_count = 0;

                // Process incoming messages
                for (port_name, message) in context.get_payload() {
                    processed_count += 1;
                    let message_str = format!("{:?}", message);
                    info!(
                        "📨 Processing message #{} from port {}: {}",
                        processed_count,
                        port_name,
                        message_str.len()
                    );

                    // Simulate some processing time
                    time::sleep(Duration::from_millis(100)).await;

                    // // Trace the message processing
                    // if let Some(tracing) = reflow_network::tracing::global_tracing() {
                    //     let _ = tracing
                    //         .trace_message_sent(
                    //             "message_processor",
                    //             "output",
                    //             "ProcessedData",
                    //             message_str.len(),
                    //         )
                    //         .await;
                    // }
                }

                info!(
                    "✅ MessageProcessor completed - processed {} messages",
                    processed_count
                );
                Ok(HashMap::new())
            })
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn create_process(
        &self,
        config: ActorConfig,
        tracing_integration: Option<TracingIntegration>,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send + 'static>> {
        let behavior = self.get_behavior();
        let inports = self.get_inports();
        let outports = self.get_outports();
        let state = Arc::new(ParkingLotMutex::new(MemoryState::default()));
        let load = self.load.clone();

        Box::pin(async move {
            use futures::StreamExt;
            let config = config.clone();
            let actor_id = config.get_node_id().clone();
            // Listen for incoming messages
            let (_, receiver) = inports;
            while let Some(payload) = receiver.stream().next().await {
                let context = ActorContext::new(
                    payload,
                    outports.clone(),
                    state.clone(),
                    config.clone(),
                    load.clone(),
                );

                if let Err(e) = behavior(context).await {
                    error!("MessageProcessor error: {}", e);
                    if let Some(tracing) = tracing_integration.clone() {
                        let _ = tracing
                            .trace_actor_failed("message_processor", e.to_string())
                            .await;
                    }
                }

                if let Some(tracing) = tracing_integration.clone() {
                    let _ = tracing.trace_actor_completed("message_processor").await;
                }
            }
        })
    }
}

// async fn setup_tracing_integration(server_url: &str, flow_id: &str) -> Result<TracingIntegration> {
//     // Create a direct tracing client for real-time events
//     let tracing_config = TracingConfig {
//         server_url: server_url.to_string(),
//         batch_size: 1, // Send events immediately for real-time monitoring
//         batch_timeout: Duration::from_millis(100),
//         enable_compression: false, // Disable compression for small events
//         retry_config: reflow_network::tracing::RetryConfig {
//             max_retries: 3,
//             initial_delay: Duration::from_millis(500),
//             max_delay: Duration::from_secs(10),
//             backoff_multiplier: 2.0,
//         },
//         enabled: true,
//     };

//     let client = TracingClient::new(tracing_config);

//     info!("🔗 Connecting to tracing server at {}", server_url.yellow());
//     client.connect().await?;
//     info!("✅ Connected to tracing server");

//     // Start a flow trace and get the trace ID
//     let flow_id = FlowId::new(flow_id);
//     let version = FlowVersion {
//         major: 1,
//         minor: 0,
//         patch: 0,
//         git_hash: None,
//         timestamp: chrono::Utc::now(),
//     };

//     // Store the client globally for easy access
//     // let _ = GLOBAL_CLIENT.set(client);

//     // let client = Arc::new(client);
//     let trace_id = client.start_trace(flow_id.clone(), version).await?;
//     info!(
//         "📋 Started flow trace: {} for flow: {}",
//         trace_id.to_string().cyan(),
//         flow_id.to_string().cyan()
//     );
//     Ok(TracingIntegration::new(client))
// }

// fn get_global_client() -> Option<&'static TracingClient> {
//     GLOBAL_CLIENT.get()
// }

async fn run_simple_actor_demo(
    count: usize,
    delay: u64,
    mut network: Network
) -> Result<()> {
    info!("🎬 Starting simple actor demonstration...");

    // Create actors
    let generator = DataGenerator::new(count);
    let processor = MessageProcessor::new();
    network.register_actor("DataGenerator", generator)?;
    network.register_actor("MessageProcessor", processor)?;
    // Create actor configs
    let generator_config = ActorConfig::default();
    let processor_config = ActorConfig::default();

    network.add_node("data_generator", "DataGenerator", None);
    network.add_node("message_processor", "MessageProcessor", None);

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "data_generator".to_owned(),
            port: "Output".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "message_processor".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

     network.add_initial(InitialPacket {
            to: ConnectionPoint {
                actor: "data_generator".to_owned(),
                port: "Input".to_owned(),
                initial_data: Some(Message::string("trigger".to_string())),
            },
    });

    network.start()?;

    // Let them run for a while
    let run_duration = Duration::from_millis(delay * count as u64 + 2000);
    info!("⏳ Running for {:?}...", run_duration);
    time::sleep(run_duration).await;

    // Stop the tasks
    network.shutdown();

    info!("🛑 Actors stopped");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!(
            "simple_client={},reflow_network={}",
            log_level, log_level
        ))
        .init();

    println!(
        "{}",
        "🚀 Simple Reflow Tracing Client Example".bold().green()
    );
    println!("Server: {}", args.server_url.yellow());
    println!("Flow ID: {}", args.flow_id.cyan());
    println!("Messages: {}", args.count.to_string().magenta());
    println!("Delay: {}ms", args.delay.to_string().blue());
    println!();

    // Setup tracing integration
    info!("🔧 Setting up tracing integration...");
    // let tracing_integration = setup_tracing_integration(&args.server_url, &args.flow_id).await?;
    let tracing_config = TracingConfig {
        server_url: args.server_url.to_string(),
        batch_size: 1, // Send events immediately for real-time monitoring
        batch_timeout: Duration::from_millis(100),
        enable_compression: false, // Disable compression for small events
        retry_config: reflow_network::tracing::RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        },
        enabled: true,
    };
    let network = Network::new(NetworkConfig{
        tracing: tracing_config, 
        ..Default::default()
    });



    // Run the actor demonstration
    run_simple_actor_demo(args.count, args.delay, network).await?;

    // Give tracing time to flush final events
    info!("📤 Flushing final trace events...");
    time::sleep(Duration::from_secs(2)).await;

    println!();
    println!("{}", "✅ Example completed successfully!".bold().green());
    println!("Check the tracing server for collected trace data.");
    println!("You can use the monitoring client to view live events:");
    println!("  {}", "cargo run --bin monitoring_client".yellow());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_generator_creation() {
        let trace_id = TraceId::new();
        let generator = DataGenerator::new(5);
        assert_eq!(generator.message_count, 5);
    }

    #[test]
    fn test_message_processor_creation() {
        let processor = MessageProcessor::new();
        // Just ensure it can be created without panics
        assert!(true);
    }

    #[tokio::test]
    async fn test_actor_behavior() {
        // Test that we can create the behavior functions
        let trace_id = TraceId::new();
        let generator = DataGenerator::new(1, trace_id);
        let behavior = generator.get_behavior();

        // This would require a proper ActorContext to test fully
        // For now, just ensure the behavior can be created
        assert!(true);
    }
}
