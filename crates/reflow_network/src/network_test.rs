use crate::{
    actor::ActorContext,
    connector::{ConnectionPoint, Connector, InitialPacket},
    network::{Network, NetworkConfig},
};
use reflow_actor_macro::actor;
use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::Value;

use crate::{
    actor::{Actor, ActorBehavior, MemoryState, Port},
    message::Message,
};

#[actor(
    SumActor,
    inports::<100>(A, B),
    outports::<100>(Out),
    await_all_inports
)]
async fn sum_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    println!("SumActor received payload: {:?}", payload);

    let _a = payload.get("A").expect("expected to get data from port A");
    let _b = payload.get("B").expect("expected to get data from port B");

    let a = match _a {
        Message::Integer(value) => *value,
        _ => 0,
    };

    let b = match _b {
        Message::Integer(value) => *value,
        _ => 0,
    };

    Ok([("Out".to_owned(), Message::Integer(a + b))].into())
}

#[actor(
    SquareActor,
    inports::<100>(In),
    outports::<50>(Out)
)]
async fn square_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let message = payload.get("In").unwrap();
    let input = match message {
        Message::Integer(value) => *value,
        _ => 0,
    };

    Ok([("Out".to_owned(), Message::Integer(input * input))].into())
}

#[actor(
    AssertEqActor,
    state(MemoryState),
    inports::<100>(A, B),
    outports(Out),
    await_all_inports
)]
async fn _assert_eq(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let data_a = payload.get("A").expect("expected to get data from port A");
    let data_b = payload.get("B").expect("expected to get data from port B");
    let a = match data_a {
        Message::Integer(value) => *value,
        _ => 0,
    };

    let b = match data_b {
        Message::Integer(value) => *value,
        _ => 0,
    };

    assert_eq!(a, b);
    println!("====================================");
    println!("|| [ASSERT_LOG]  {} == {}         ||", a, b);
    println!("====================================");

    Ok([].into())
}

#[tokio::test]
async fn test_network() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    let sum_id = "Sum";
    let square_id = "Square";
    let asser_eq_id = "AssertEq";

    network.register_actor("sum_process", SumActor::new())?;

    network.register_actor("square_process", SquareActor::new())?;

    network.register_actor("assert_eq_process", AssertEqActor::new())?;

    network.add_node(sum_id, "sum_process", None)?;
    network.add_node(square_id, "square_process", None)?;
    network.add_node(asser_eq_id, "assert_eq_process", None)?;

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: sum_id.to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: square_id.to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: square_id.to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: asser_eq_id.to_owned(),
            port: "A".to_owned(),
            ..Default::default()
        },
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: sum_id.to_owned(),
            port: "A".to_owned(),
            initial_data: Some(Message::Integer(2)),
        },
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: sum_id.to_owned(),
            port: "B".to_owned(),
            initial_data: Some(Message::Integer(3)),
        },
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: asser_eq_id.to_owned(),
            port: "B".to_owned(),
            initial_data: Some(Message::Integer(25)),
        },
    });

    // Start the network
    network.start()?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    network.shutdown();

    Ok(())
}

// Data transformation actor
#[actor(
    TransformActor,
    inports::<100>(Input),
    outports::<50>(Output),
    state(MemoryState)
)]
async fn transform_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = payload.get("Input").expect("expected Input data");
    let count = {
        let mut count = 0;
        let mut state = state.lock();
        if let Some(state) = state.as_mut_any().downcast_mut::<MemoryState>() {
            count = state
                .get("count")
                .unwrap_or(&Value::Number(0.into()))
                .as_i64()
                .unwrap_or(0);
            state.insert("count", Value::Number((count + 1).into()));
        }
        count
    };

    let result = match input {
        Message::Integer(n) => Message::Integer(n + count),
        Message::String(s) => Message::string(format!("{}{}", s, count)),
        _ => Message::any(Value::Null.into()),
    };

    Ok([("Output".to_owned(), result)].into())
}

// Filtering actor
#[actor(
    FilterActor,
    inports::<100>(In),
    outports::<50>(Passed, Failed)
)]
async fn filter_actor(
    context: ActorContext,
) -> Result<HashMap<std::string::String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let input = payload.get("In").expect("expected input");

    match input {
        Message::Integer(n) if *n > 0 => Ok([("Passed".to_owned(), input.clone())].into()),
        _ => Ok([("Failed".to_owned(), input.clone())].into()),
    }
}

// Aggregator actor
#[actor(
    AggregatorActor,
    inports::<100>(Value),
    outports::<50>(Sum, Count),
    state(MemoryState)
)]
async fn aggregator_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let value = payload.get("Value").expect("expected Value");
    let mut result = HashMap::new();
    let mut sum = 0;
    let mut count = 0;
    let mut state = state.lock();
    if let Some(state) = state.as_mut_any().downcast_mut::<MemoryState>() {
        sum = state
            .get("sum")
            .unwrap_or(&Value::Number(0.into()))
            .as_i64()
            .unwrap_or(0);
        count = state
            .get("count")
            .unwrap_or(&Value::Number(0.into()))
            .as_i64()
            .unwrap_or(0);

        if let Message::Integer(n) = value {
            sum += n;
            count += 1;
        }

        state.insert("sum", Value::Number(sum.into()));
        state.insert("count", Value::Number(count.into()));
    }
    result.insert("Sum".to_owned(), Message::Integer(sum));
    result.insert("Count".to_owned(), Message::Integer(count));

    println!("AggregatorActor: Sum = {}, Count = {}", sum, count);

    Ok(result)
}

#[tokio::test]
async fn test_complex_network() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    // Register actors
    network.register_actor("transform", TransformActor::new())?;
    network.register_actor("filter", FilterActor::new())?;
    network.register_actor("aggregator", AggregatorActor::new())?;

    // Add nodes
    network.add_node("transform1", "transform", None)?;
    network.add_node("filter1", "filter", None)?;
    network.add_node("aggregator1", "aggregator", None)?;

    // Connect nodes
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "transform1".to_owned(),
            port: "Output".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "filter1".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "filter1".to_owned(),
            port: "Passed".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "aggregator1".to_owned(),
            port: "Value".to_owned(),
            ..Default::default()
        },
    });

    // Add initial values
    for i in 1..=5 {
        network.add_initial(InitialPacket {
            to: ConnectionPoint {
                actor: "transform1".to_owned(),
                port: "Input".to_owned(),
                initial_data: Some(Message::Integer(i)),
            },
        });
    }

    // Start network
    network.start()?;

    tokio::time::sleep(Duration::from_secs(2)).await;
    network.shutdown();
    Ok(())
}

// Forward actor: receives on "In", sends same message on "Out"
#[actor(
    ForwardActor,
    inports::<100>(In),
    outports::<100>(Out)
)]
async fn forward_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let input = payload.get("In").expect("expected input on In port");
    Ok([("Out".to_owned(), input.clone())].into())
}

/// Test fan-out: one actor's outport connected to two downstream actors.
/// With broadcast, both collectors should receive every message.
#[tokio::test]
async fn test_fanout_broadcast() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    // Source actor transforms input, two forward actors receive the output
    network.register_actor("transform", TransformActor::new())?;
    network.register_actor("forward_a", ForwardActor::new())?;
    network.register_actor("forward_b", ForwardActor::new())?;

    network.add_node("source", "transform", None)?;
    network.add_node("sink_a", "forward_a", None)?;
    network.add_node("sink_b", "forward_b", None)?;

    // Fan-out: source:Output → sink_a:In AND source:Output → sink_b:In
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "source".to_owned(),
            port: "Output".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "sink_a".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "source".to_owned(),
            port: "Output".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "sink_b".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    // Send 3 messages to the source
    for i in 1..=3 {
        network.add_initial(InitialPacket {
            to: ConnectionPoint {
                actor: "source".to_owned(),
                port: "Input".to_owned(),
                initial_data: Some(Message::Integer(i)),
            },
        });
    }

    network.start()?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Both forward actors should have received all 3 messages on
    // their outports. Read from `initialized_actors` keyed by *node
    // id* (sink_a / sink_b), not from `actors` keyed by template
    // name — the runtime calls `create_instance` per node, so the
    // template's channels are unrelated to the running node's.
    let forward_a_outport = network
        .initialized_actors
        .get("sink_a")
        .unwrap()
        .get_outports()
        .1;
    let forward_b_outport = network
        .initialized_actors
        .get("sink_b")
        .unwrap()
        .get_outports()
        .1;

    let mut a_messages = Vec::new();
    while let Ok(pkt) = forward_a_outport.try_recv() {
        if let Some(msg) = pkt.get("Out") {
            a_messages.push(msg.clone());
        }
    }

    let mut b_messages = Vec::new();
    while let Ok(pkt) = forward_b_outport.try_recv() {
        if let Some(msg) = pkt.get("Out") {
            b_messages.push(msg.clone());
        }
    }

    println!(
        "forward_a received {} messages: {:?}",
        a_messages.len(),
        a_messages
    );
    println!(
        "forward_b received {} messages: {:?}",
        b_messages.len(),
        b_messages
    );

    // With broadcast fan-out, BOTH should receive all 3 messages
    assert_eq!(
        a_messages.len(),
        3,
        "forward_a should have received all 3 messages (got {})",
        a_messages.len()
    );
    assert_eq!(
        b_messages.len(),
        3,
        "forward_b should have received all 3 messages (got {})",
        b_messages.len()
    );

    network.shutdown();
    Ok(())
}

// ============================================================
// Actor-to-actor streaming via the network
// ============================================================

/// Produces a StreamHandle containing 3 binary chunks and sends it
/// through the normal outport.  A real actor would push the frames
/// concurrently; here we do it inside the behavior closure because the
/// test sleeps long enough for everything to drain.
#[actor(
    StreamProducerActor,
    inports::<10>(Trigger),
    outports::<10>(Out)
)]
async fn stream_producer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    let (tx, handle) = context.create_stream(
        "Out",
        Some("application/octet-stream".into()),
        Some(300),
        Some(8),
    );

    // Push frames (Begin → Data × 3 → End)
    tx.send_async(StreamFrame::Begin {
        content_type: Some("application/octet-stream".into()),
        size_hint: Some(300),
        metadata: None,
    })
    .await
    .unwrap();

    for chunk in [
        b"chunk-1".to_vec(),
        b"chunk-2".to_vec(),
        b"chunk-3".to_vec(),
    ] {
        tx.send_async(StreamFrame::Data(Arc::new(chunk)))
            .await
            .unwrap();
    }

    tx.send_async(StreamFrame::End).await.unwrap();

    // The handle travels through the normal connector
    Ok([("Out".to_owned(), Message::stream_handle(handle))].into())
}

/// Receives a StreamHandle on its inport, reads all frames from the
/// side-channel, reassembles the data, and outputs the total byte count
/// so the test harness can verify.
#[actor(
    StreamConsumerActor,
    inports::<10>(In),
    outports::<10>(ByteCount)
)]
async fn stream_consumer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    let rx = context
        .take_stream_receiver("In")
        .expect("expected stream receiver on In port");

    let mut total_bytes: usize = 0;
    let mut got_begin = false;
    let mut got_end = false;

    loop {
        match rx.recv_async().await {
            Ok(StreamFrame::Begin { .. }) => got_begin = true,
            Ok(StreamFrame::Data(data)) => total_bytes += data.len(),
            Ok(StreamFrame::End) => {
                got_end = true;
                break;
            }
            Ok(StreamFrame::Error(e)) => return Err(anyhow::anyhow!("Stream error: {}", e)),
            Err(_) => break,
        }
    }

    assert!(got_begin, "consumer should have received Begin frame");
    assert!(got_end, "consumer should have received End frame");

    Ok([("ByteCount".to_owned(), Message::Integer(total_bytes as i64))].into())
}

/// Full network integration: producer → connector → consumer, with
/// streaming data flowing through the side-channel while the
/// StreamHandle travels through the normal connector path.
#[tokio::test]
async fn test_network_actor_streaming() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    network.register_actor("stream_producer", StreamProducerActor::new())?;
    network.register_actor("stream_consumer", StreamConsumerActor::new())?;

    network.add_node("producer", "stream_producer", None)?;
    network.add_node("consumer", "stream_consumer", None)?;

    // Wire producer:Out → consumer:In
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "producer".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "consumer".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    // Trigger the producer
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "producer".to_owned(),
            port: "Trigger".to_owned(),
            initial_data: Some(Message::Flow),
        },
    });

    network.start()?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Read the consumer's outport to verify. Keyed by node id
    // ("consumer"), not template name — the runtime gives every node
    // its own channels via `create_instance`.
    let consumer_outport = network
        .initialized_actors
        .get("consumer")
        .unwrap()
        .get_outports()
        .1;

    let mut byte_counts = Vec::new();
    while let Ok(pkt) = consumer_outport.try_recv() {
        if let Some(Message::Integer(n)) = pkt.get("ByteCount") {
            byte_counts.push(*n);
        }
    }

    // "chunk-1" (7) + "chunk-2" (7) + "chunk-3" (7) = 21 bytes
    assert_eq!(
        byte_counts,
        vec![21],
        "consumer should have received 21 total bytes from the stream"
    );

    network.shutdown();
    Ok(())
}

/// Fan-out streaming: one producer's StreamHandle is broadcast to two
/// consumers.  Since StreamHandle is single-consumer (one receiver per
/// stream ID), only the first consumer that calls take_stream_receiver
/// gets the data.  The second consumer should gracefully handle the
/// missing receiver.
#[actor(
    StreamConsumerGracefulActor,
    inports::<10>(In),
    outports::<10>(ByteCount)
)]
async fn stream_consumer_graceful_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    match context.take_stream_receiver("In") {
        Some(rx) => {
            let mut total_bytes: usize = 0;
            loop {
                match rx.recv_async().await {
                    Ok(StreamFrame::Data(data)) => total_bytes += data.len(),
                    Ok(StreamFrame::End) | Err(_) => break,
                    _ => {}
                }
            }
            Ok([("ByteCount".to_owned(), Message::Integer(total_bytes as i64))].into())
        }
        None => {
            // No receiver available — another consumer took it
            Ok([("ByteCount".to_owned(), Message::Integer(-1))].into())
        }
    }
}

#[tokio::test]
async fn test_network_stream_fanout_single_consumer() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    network.register_actor("stream_producer", StreamProducerActor::new())?;
    network.register_actor("consumer_a", StreamConsumerGracefulActor::new())?;
    network.register_actor("consumer_b", StreamConsumerGracefulActor::new())?;

    network.add_node("producer", "stream_producer", None)?;
    network.add_node("sink_a", "consumer_a", None)?;
    network.add_node("sink_b", "consumer_b", None)?;

    // Fan-out: producer:Out → sink_a:In AND producer:Out → sink_b:In
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "producer".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "sink_a".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "producer".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "sink_b".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "producer".to_owned(),
            port: "Trigger".to_owned(),
            initial_data: Some(Message::Flow),
        },
    });

    network.start()?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Collect results from both consumers. Keys are *node ids*
    // (sink_a / sink_b), not template names — `create_instance`
    // gives each running node its own channels.
    let mut results: Vec<i64> = Vec::new();
    for name in ["sink_a", "sink_b"] {
        let outport = network
            .initialized_actors
            .get(name)
            .unwrap()
            .get_outports()
            .1;
        while let Ok(pkt) = outport.try_recv() {
            if let Some(Message::Integer(n)) = pkt.get("ByteCount") {
                results.push(*n);
            }
        }
    }

    results.sort();
    // One consumer gets the stream (21 bytes), the other gets -1 (no receiver)
    assert_eq!(
        results,
        vec![-1, 21],
        "One consumer should get 21 bytes, the other -1 (single-consumer stream)"
    );

    network.shutdown();
    Ok(())
}
