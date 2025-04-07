use std::{collections::HashMap, pin::Pin, sync::Arc, thread::sleep, time::Duration};

use crate::{
    actor::ActorPayload,
    connector::{ConnectionPoint, Connector, InitialPacket},
    network::{Network, NetworkConfig},
};
use actor_macro::actor;
use parking_lot::Mutex;
use serde_json::{Map, Value};

use crate::{
    actor::{Actor, ActorBehavior, ActorState, MemoryState, Port},
    message::Message,
};

#[actor(
    SumActor,
    inports::<100>(A, B),
    outports::<100>(Out),
    await_all_inports
)]
async fn sum_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, anyhow::Error> {
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

    return Ok([("Out".to_owned(), Message::Integer(a + b))].into());
}

#[actor(
    SquareActor,
    inports::<100>(In),
    outports::<50>(Out)
)]
async fn square_actor(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, anyhow::Error> {
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
async fn _assert_eq(
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, anyhow::Error> {
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

#[test]
fn test_network() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    let sum_id = "Sum";
    let square_id = "Square";
    let asser_eq_id = "AssertEq";

    network.register_actor("sum_process", SumActor::new())?;

    network.register_actor("square_process", SquareActor::new())?;

    network.register_actor("assert_eq_process", AssertEqActor::new())?;

    network.add_node(sum_id, "sum_process")?;
    network.add_node(square_id, "square_process")?;
    network.add_node(asser_eq_id, "assert_eq_process")?;

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

    Ok(())
}

// Data transformation actor
#[actor(
    TransformActor,
    inports::<100>(Input),
    outports::<50>(Output),
    state(MemoryState)
)]
async fn transform_actor(
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, anyhow::Error> {
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
        Message::String(s) => Message::String(format!("{}{}", s, count)),
        _ => Message::Any(Value::Null.into()),
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
    payload: ActorPayload,
    _state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<std::string::String, Message>, anyhow::Error> {
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
    payload: ActorPayload,
    state: Arc<Mutex<dyn ActorState>>,
    _outport_channels: Port,
) -> Result<HashMap<String, Message>, anyhow::Error> {
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

    Ok(result)
}

#[test]
fn test_complex_network() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());

    // Register actors
    network.register_actor("transform", TransformActor::new())?;
    network.register_actor("filter", FilterActor::new())?;
    network.register_actor("aggregator", AggregatorActor::new())?;

    // Add nodes
    network.add_node("transform1", "transform")?;
    network.add_node("filter1", "filter")?;
    network.add_node("aggregator1", "aggregator")?;

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
    sleep(Duration::from_millis(100));

    Ok(())
}
