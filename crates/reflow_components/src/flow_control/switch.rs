//! Switch/case routing actor for multi-way branching.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use std::collections::HashMap;

/// Switch Case Actor - Compatible with tpl_switch
///
/// Routes data to different outputs based on case matching.
#[actor(
    SwitchCaseActor,
    inports::<100>(data),
    outports::<50>(case1, case2, case3, case4, default),
    state(MemoryState)
)]
pub async fn switch_case_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();

    let data = payload
        .get("data")
        .ok_or_else(|| anyhow::anyhow!("No input data provided"))?;

    let switch_field = config
        .get("switch_field")
        .or_else(|| config.get("field"))
        .and_then(|v| v.as_str());

    let switch_value = if let Some(field) = switch_field {
        if let Message::Object(obj) = data {
            let obj_value = serde_json::to_value(obj)?;
            obj_value.get(field).cloned()
        } else {
            None
        }
    } else {
        Some(serde_json::to_value(data)?)
    };

    let case1_value = config.get("case1_value");
    let case2_value = config.get("case2_value");
    let case3_value = config.get("case3_value");
    let case4_value = config.get("case4_value");

    let output_port = match switch_value {
        Some(val) if case1_value == Some(&val) => "case1",
        Some(val) if case2_value == Some(&val) => "case2",
        Some(val) if case3_value == Some(&val) => "case3",
        Some(val) if case4_value == Some(&val) => "case4",
        _ => "default",
    };

    result.insert(output_port.to_string(), data.clone());
    Ok(result)
}
