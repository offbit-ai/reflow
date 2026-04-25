//! Test fixture pack for `reflow_pack_loader` roundtrip tests.
//!
//! Registers `reflow.test.echo` — an actor whose behavior copies
//! `input` → `output` unchanged. Deliberately hand-rolled (no
//! `#[actor]` macro) so the fixture demonstrates the bare minimum a
//! pack needs.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use reflow_pack_sdk::{
    Actor, ActorBehavior, ActorContext, ActorLoad, Message, PackHost, Port, reflow_pack,
};
use reflow_pack_sdk::{ActorState, MemoryState};

struct EchoActor {
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl EchoActor {
    fn new() -> Self {
        Self {
            inports: flume::bounded(16),
            outports: flume::bounded(16),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

type BehaviorFut =
    Pin<Box<dyn Future<Output = Result<HashMap<String, Message>, anyhow::Error>> + Send + 'static>>;

impl Actor for EchoActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|ctx: ActorContext| -> BehaviorFut {
            Box::pin(async move {
                let payload = ctx.get_payload().clone();
                let input = payload.get("input").cloned().unwrap_or(Message::Flow);
                let mut out = HashMap::new();
                out.insert("output".to_string(), input);
                Ok(out)
            })
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn inport_names(&self) -> Vec<String> {
        vec!["input".to_string()]
    }

    fn outport_names(&self) -> Vec<String> {
        vec!["output".to_string()]
    }

    fn create_state(&self) -> Arc<Mutex<dyn ActorState>> {
        Arc::new(Mutex::new(MemoryState::default()))
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        Arc::clone(&self.load)
    }

    fn create_instance(&self) -> Arc<dyn Actor> {
        Arc::new(EchoActor::new())
    }
}

#[reflow_pack]
fn register(host: &mut PackHost) {
    host.register("reflow.test.echo", || Arc::new(EchoActor::new()));
}
