//! Distributed Network Example
//!
//! This example demonstrates how to set up and use distributed networks
//! for cross-network actor communication.

use reflow_network::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    distributed_network::{DistributedConfig, DistributedNetwork},
    message::Message,
    network::NetworkConfig, tracing::TracingIntegration,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::sleep;

// Bidirectional communication actor that can both send and respond to messages
struct BidirectionalActor {
    prefix: String,
    inports: Port,
    outports: Port,
    load: Arc<parking_lot::Mutex<ActorLoad>>,
}

impl BidirectionalActor {
    fn new(prefix: String) -> Self {
        Self {
            prefix,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(parking_lot::Mutex::new(
                reflow_network::actor::ActorLoad::new(0),
            )),
        }
    }
}

impl Actor for BidirectionalActor {
    fn get_behavior(&self) -> reflow_network::actor::ActorBehavior {
        let prefix = self.prefix.clone();

        Box::new(move |context| {
            let prefix = prefix.clone();

            Box::pin(async move {
                let payload = context.get_payload();
                let mut output = HashMap::new();

                for (port, message) in payload.iter() {
                    match port.as_str() {
                        "input" => {
                            let response = match message {
                                Message::String(s) => {
                                    tracing::info!("[{}] Received: {}", prefix, s);
                                    Message::String(format!("[{}] Processed: {}", prefix, s).into())
                                }
                                _ => {
                                    tracing::info!("[{}] Received message: {:?}", prefix, message);
                                    Message::String(
                                        format!("[{}] Processed: {:?}", prefix, message).into(),
                                    )
                                }
                            };
                            output.insert("output".to_string(), response);
                        }
                        "response" => {
                            // Handle response messages
                            match message {
                                Message::String(s) => {
                                    tracing::info!("[{}] 🎉 Received response: {}", prefix, s);
                                }
                                _ => {
                                    tracing::info!(
                                        "[{}] 🎉 Received response: {:?}",
                                        prefix,
                                        message
                                    );
                                }
                            }
                        }
                        _ => {
                            tracing::debug!("[{}] Unhandled port: {}", prefix, port);
                        }
                    }
                }

                Ok(output)
            })
        })
    }

    fn get_inports(&self) -> reflow_network::actor::Port {
        self.inports.clone()
    }

    fn get_outports(&self) -> reflow_network::actor::Port {
        self.outports.clone()
    }

    fn load_count(&self) -> Arc<parking_lot::Mutex<reflow_network::actor::ActorLoad>> {
        self.load.clone()
    }

    fn create_process(
        &self,
        actor_config: ActorConfig,
        tracing_integration:Option<TracingIntegration>
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        use futures::StreamExt;

        let behavior = self.get_behavior();
        let (_, receiver) = self.get_inports();
        let outports = self.get_outports();
        let load = self.load_count();
        let prefix = self.prefix.clone();

        Box::pin(async move {
            tracing::info!(
                "[{}] Actor process started, waiting for messages...",
                prefix
            );

            loop {
                tracing::debug!("[{}] Waiting for next message...", prefix);

                if let Some(packet) = receiver.stream().next().await {
                    tracing::info!(
                        "[{}] 📨 Received packet with {} messages",
                        prefix,
                        packet.len()
                    );

                    let context = ActorContext::new(
                        packet,
                        outports.clone(),
                        Arc::new(parking_lot::Mutex::new(MemoryState::default())),
                        actor_config.clone(),
                        load.clone(),
                    );

                    if let Ok(result) = behavior(context).await {
                        if !result.is_empty() {
                            tracing::info!(
                                "[{}] 📤 Sending {} output messages",
                                prefix,
                                result.len()
                            );
                            let _ = outports
                                .0
                                .send(result)
                                .expect("Expected to send message via outport");
                            load.lock().reset();
                        } else {
                            tracing::debug!("[{}] No output messages to send", prefix);
                        }
                    } else {
                        tracing::error!("[{}] Error processing messages", prefix);
                    }
                } else {
                    tracing::warn!(
                        "[{}] Received None from message stream, actor stopping",
                        prefix
                    );
                    break;
                }
            }

            tracing::info!("[{}] Actor process ended", prefix);
        })
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Starting Distributed Network Example");

    // Configure Server Network
    let server_config = DistributedConfig {
        network_id: "server_network".to_string(),
        instance_id: "server_001".to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: 9090,
        discovery_endpoints: vec![],
        auth_token: Some("server_token".to_string()),
        max_connections: 10,
        heartbeat_interval_ms: 30000,
        local_network_config: NetworkConfig::default(),
    };

    // Configure Client Network
    let client_config = DistributedConfig {
        network_id: "client_network".to_string(),
        instance_id: "client_001".to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: 9091,
        discovery_endpoints: vec![],
        auth_token: Some("client_token".to_string()),
        max_connections: 10,
        heartbeat_interval_ms: 30000,
        local_network_config: NetworkConfig::default(),
    };

    // Create distributed networks
    let mut server_network = DistributedNetwork::new(server_config).await?;
    let mut client_network = DistributedNetwork::new(client_config).await?;

    println!("📡 Created server and client networks");

    println!("🌐 Started server and client networks");

    // Give networks time to initialize
    sleep(Duration::from_secs(1)).await;

    // Register local actors on server
    {
        let server_actor = BidirectionalActor::new("SERVER".to_string());
        server_network.register_local_actor("server_actor", server_actor, None)?;
        // Start networks
        server_network.start().await?;
    }

    // Register local actors on client
    {
        let client_actor = BidirectionalActor::new("CLIENT".to_string());
        client_network.register_local_actor("client_actor", client_actor, None)?;
        client_network.start().await?;
    }

    println!("🎭 Registered local actors on both networks");

    // *** ESTABLISH CONNECTION BETWEEN NETWORKS ***
    println!("🔌 Establishing connection from client to server...");
    client_network.connect_to_network("127.0.0.1:9090").await?;

    // Give connection time to establish
    sleep(Duration::from_secs(1)).await;
    println!("✅ Connection established!");

    // Register remote actors (server's actors on client)
    client_network
        .register_remote_actor("server_actor", "server_network")
        .await?;
    println!("🔗 Registered remote actor on client");

    // *** BIDIRECTIONAL COMMUNICATION TEST ***

    // 1. Client sends message to server through proxy actor
    println!("\n🚀 Step 1: Client → Server (via proxy)");
    let test_message = Message::String("Hello from client!".to_string().into());

    // Send through the local network to the proxy actor
    {
        let client_net = client_network.get_local_network();
        let net = client_net.read();
        net.send_to_actor("server_actor@server_network", "input", test_message)?;
    }
    println!("📤 Client sent message to server via proxy");

    // Give more time for processing
    sleep(Duration::from_secs(5)).await;

    // 2. Server sends message to client
    println!("\n🚀 Step 2: Server → Client");
    server_network
        .register_remote_actor("client_actor", "client_network")
        .await?;

    // Give time for registration
    sleep(Duration::from_secs(2)).await;

    

    let response_message = Message::String("Hello from server!".to_string().into());
    {
        let server_net = server_network.get_local_network();
        let net = server_net.read();
        net.send_to_actor("client_actor@client_network", "input", response_message)?;
    }
    println!("📤 Server sent message to client via proxy");

    // Give much more time for final processing
    sleep(Duration::from_secs(10)).await;

    println!("✅ Distributed network example completed successfully!");
    println!("   - Server network running on 127.0.0.1:9080");
    println!("   - Client network running on 127.0.0.1:8081");
    println!("   - Cross-network communication established");

    // Shutdown networks properly
    println!("\n🔽 Shutting down networks...");

    // Shutdown client network first
    if let Err(e) = client_network.shutdown().await {
        println!("⚠️  Error shutting down client network: {}", e);
    } else {
        println!("✅ Client network shut down successfully");
    }

    // Shutdown server network
    if let Err(e) = server_network.shutdown().await {
        println!("⚠️  Error shutting down server network: {}", e);
    } else {
        println!("✅ Server network shut down successfully");
    }

    println!("🎯 All networks shut down cleanly!");

    Ok(())
}
