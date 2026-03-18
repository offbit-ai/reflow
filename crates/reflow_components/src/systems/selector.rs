//! Entity selector — shared logic for resolving which entities a system operates on.
//!
//! Every system must explicitly select its entities via:
//!
//! 1. `entity` — single entity ID (from config or inport)
//! 2. `entities` — explicit list of entity IDs (from config)
//! 3. `selector` — tag/query filter: `{ "tags": ["enemy"], "type": "rigidbody" }`
//!
//! If none provided, returns empty — systems don't implicitly touch everything.

use reflow_actor::message::Message;
use reflow_assets::{AssetDB, AssetQuery};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Resolve which entities this system invocation should process.
///
/// Checks (in order): inport `entity_id`, config `entity`, config `entities`,
/// config `selector`. Returns empty vec if nothing is specified.
pub fn resolve_entities(
    payload: &HashMap<String, Message>,
    config: &HashMap<String, Value>,
    db: &Arc<AssetDB>,
) -> Vec<String> {
    // 1. Single entity from inport
    if let Some(Message::String(s)) = payload.get("entity_id") {
        return vec![s.to_string()];
    }

    // 2. Single entity from config
    if let Some(id) = config.get("entity").and_then(|v| v.as_str()) {
        return vec![id.to_string()];
    }

    // 3. Explicit list from config
    if let Some(arr) = config.get("entities").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    // 4. Query selector from config
    if let Some(selector) = config.get("selector") {
        let q = AssetQuery::from_json(selector);
        if let Ok(entries) = db.query(&q) {
            // Extract entity names from matched entries
            return entries
                .iter()
                .map(|e| e.name.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
        }
    }

    // Nothing specified — return empty. Systems should not default to "all".
    Vec::new()
}
