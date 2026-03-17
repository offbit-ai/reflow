//! Expression system — evaluates expression components each tick.
//!
//! Uses `reflow_assets::expression` (the shared evaluator also used by
//! MathExpressionActor). The system reads `:expression` components from
//! AssetDB, resolves variables from other components, evaluates, and
//! writes results to target properties.
//!
//! Math actors (MathAdd, MathMultiply, etc.) can also drive these
//! expressions — wire them into the DAG to compute variables, then
//! the expression system reads the computed values from AssetDB.
//!
//! ## Component schema: `entity:expression`
//!
//! ```json
//! {
//!   "target": "particle:transform.rotation.z",
//!   "expr": "time * 360",
//!   "vars": {
//!     "time": "clock:time.elapsed",
//!     "speed": 2.0
//!   }
//! }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use crate::math::expression as expr_eval;
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    ExpressionSystemActor,
    inports::<10>(tick),
    outports::<1>(metadata),
    state(MemoryState)
)]
pub async fn expression_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let db_path = config.get("$db").and_then(|v| v.as_str()).unwrap_or("./assets.db");

    let db = get_or_create_db(db_path)?;
    let expr_entries = db.query(&reflow_assets::AssetQuery::new().asset_type("expression"))?;
    let mut evaluated = 0;

    for entry in &expr_entries {
        let data = match &entry.inline_data {
            Some(v) => v.clone(),
            None => continue,
        };

        let expr_str = match data.get("expr").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        let target = match data.get("target").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Resolve variables: constants + component paths + literal values
        let mut vars: HashMap<String, f64> = HashMap::new();
        if let Some(var_map) = data.get("vars").and_then(|v| v.as_object()) {
            for (name, path_val) in var_map {
                if let Some(path) = path_val.as_str() {
                    if let Some(val) = resolve_path(&db, path) {
                        vars.insert(name.clone(), val);
                    }
                } else if let Some(n) = path_val.as_f64() {
                    vars.insert(name.clone(), n);
                }
            }
        }

        // Evaluate using the shared expression engine
        if let Some(result) = expr_eval::eval(expr_str, &vars) {
            write_target(&db, &target, &json!(result));
            evaluated += 1;
        }
    }

    let mut out = HashMap::new();
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "expressionsEvaluated": evaluated,
        }))),
    );
    Ok(out)
}

fn resolve_path(db: &std::sync::Arc<reflow_assets::AssetDB>, path: &str) -> Option<f64> {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];
    let field_path = parts.get(1).copied();

    let asset = db.get(entity_component).ok()?;
    let v: Value = if let Some(ref inline) = asset.entry.inline_data {
        inline.clone()
    } else {
        serde_json::from_slice(&asset.data).ok()?
    };

    let target = if let Some(fp) = field_path {
        let mut current = &v;
        for key in fp.split('.') {
            current = current.get(key)?;
        }
        current.clone()
    } else {
        v
    };

    target.as_f64()
}

fn write_target(db: &std::sync::Arc<reflow_assets::AssetDB>, path: &str, value: &Value) {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];

    if parts.len() == 1 {
        let _ = db.put_json(entity_component, value.clone(), json!({}));
        return;
    }

    let field_path = parts[1];
    if let Ok(asset) = db.get(entity_component) {
        let mut current: Value = if let Some(ref inline) = asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&asset.data).unwrap_or(json!({}))
        };
        set_json_path(&mut current, field_path, value.clone());
        let _ = db.put_json(entity_component, current, asset.entry.metadata);
    }
}

fn set_json_path(obj: &mut Value, path: &str, value: Value) {
    let keys: Vec<&str> = path.split('.').collect();
    let mut current = obj;
    for (i, key) in keys.iter().enumerate() {
        if i == keys.len() - 1 {
            current[key] = value;
            return;
        }
        if !current.get(key).map(|v| v.is_object()).unwrap_or(false) {
            current[key] = json!({});
        }
        current = &mut current[key];
    }
}
