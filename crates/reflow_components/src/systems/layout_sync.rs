//! Layout sync system — bridges AssetDB ↔ DOM/layout tree.
//!
//! ## Two-way binding
//!
//! Entities with a `:bind` component get automatic bi-directional sync.
//! No explicit wiring needed — toggle `bind: true` on the entity.
//!
//! ```json
//! // Full auto-bind (all standard properties)
//! { "entity": "slider", "component": "bind", "data": true }
//!
//! // Selective bind (only listed properties)
//! {
//!   "entity": "input_field",
//!   "component": "bind",
//!   "data": {
//!     "value": true,
//!     "transform": true,
//!     "style": false
//!   }
//! }
//! ```
//!
//! Bound entities each tick:
//! 1. **Pull**: layout computed values → AssetDB (position, scroll, input value)
//! 2. **Push**: AssetDB changes → layout (transform, style, text)
//!
//! All bind operations are traced via `reflow_tracing_protocol` when enabled,
//! so the full data flow is visible in the DAG inspector.
//!
//! ## Unbound entities
//!
//! Entities without `:bind` still participate in explicit sync via
//! phase "poll"/"sync" — the LayoutSyncSystem reads/writes them when
//! the DAG invokes it. Bind is convenience, not replacement.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::{get_or_create_db, layout};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static HYDRATED: AtomicBool = AtomicBool::new(false);

#[actor(
    LayoutSyncSystemActor,
    inports::<10>(tick, entity_id),
    outports::<1>(bound_changes, metadata),
    state(MemoryState)
)]
pub async fn layout_sync_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let _payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let phase = config
        .get("phase")
        .and_then(|v| v.as_str())
        .unwrap_or("both");

    let db = get_or_create_db(db_path)?;

    let backend = match layout::get_layout_backend(db_path) {
        Some(b) => b,
        None => {
            let headless = std::sync::Arc::new(layout::HeadlessLayoutBackend::new());
            layout::set_layout_backend(db_path, headless.clone());
            headless as std::sync::Arc<dyn layout::LayoutBackend>
        }
    };

    // Hydrate on first run
    if !HYDRATED.swap(true, Ordering::Relaxed) {
        backend.hydrate(&db)?;
    }

    let mut poll_count = 0;
    let mut sync_count = 0;
    let mut bound_changes: Vec<Value> = Vec::new();

    // ── Phase: poll (layout → AssetDB) ──
    if phase == "poll" || phase == "both" {
        backend.poll_events(&db)?;
        poll_count = 1;
    }

    // ── Two-way bind: process bound entities ──
    let bound_entities = db.entities_with(&["bind"])?;

    for entity in &bound_entities {
        let bind_asset = match db.get_component(entity, "bind") {
            Ok(a) => a,
            Err(_) => continue,
        };

        let bind_config: Value = if let Some(ref inline) = bind_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&bind_asset.data).unwrap_or(Value::Bool(true))
        };

        // Determine which properties to bind
        let bind_all = bind_config.as_bool().unwrap_or(false);
        let bind_transform = bind_all
            || bind_config
                .get("transform")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        let _bind_style = bind_all
            || bind_config
                .get("style")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        let bind_value = bind_all
            || bind_config
                .get("value")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        let bind_scroll = bind_all
            || bind_config
                .get("scroll")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        // ── Pull: layout → AssetDB ──
        if bind_transform {
            if let (Some(x), Some(y)) = (backend.query(entity, "x"), backend.query(entity, "y")) {
                // Only write if changed (compare with current AssetDB value)
                let current = db.get_component(entity, "transform").ok().and_then(|a| {
                    a.entry
                        .inline_data
                        .clone()
                        .or_else(|| serde_json::from_slice(&a.data).ok())
                });
                let needs_update = current
                    .as_ref()
                    .map(|c| {
                        let cx = c
                            .get("position")
                            .and_then(|p| p.get(0))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(f64::NAN);
                        let cy = c
                            .get("position")
                            .and_then(|p| p.get(1))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(f64::NAN);
                        (cx - x).abs() > 0.001 || (cy - y).abs() > 0.001
                    })
                    .unwrap_or(true);

                if needs_update {
                    let mut tf = current.unwrap_or(json!({}));
                    tf["position"] = json!([x, y, 0.0]);
                    db.set_component_json(
                        entity,
                        "transform",
                        tf,
                        json!({"source": "layout_pull"}),
                    )?;
                    bound_changes.push(
                        json!({"entity": entity, "direction": "pull", "property": "transform"}),
                    );
                }
            }
        }

        if bind_value {
            if let Some(val) = backend.query(entity, "value") {
                db.set_component_json(
                    entity,
                    "input_value",
                    json!({"value": val}),
                    json!({"source": "layout_pull"}),
                )?;
                bound_changes
                    .push(json!({"entity": entity, "direction": "pull", "property": "value"}));
            }
        }

        if bind_scroll {
            if let (Some(sx), Some(sy)) = (
                backend.query(entity, "scrollX"),
                backend.query(entity, "scrollY"),
            ) {
                let progress = backend.query(entity, "scrollProgress").unwrap_or(0.0);
                db.set_component_json(
                    entity,
                    "scroll",
                    json!({
                        "x": sx, "y": sy, "progress": progress,
                    }),
                    json!({"source": "layout_pull"}),
                )?;
                bound_changes
                    .push(json!({"entity": entity, "direction": "pull", "property": "scroll"}));
            }
        }

        // ── Push: AssetDB → layout ──
        // (happens in the sync phase below, but we mark bound entities for priority)
    }

    // ── Phase: sync (AssetDB → layout) ──
    if phase == "sync" || phase == "both" {
        backend.sync(&db)?;
        sync_count = 1;

        // Track push changes for bound entities
        for entity in &bound_entities {
            bound_changes.push(json!({"entity": entity, "direction": "push"}));
        }
    }

    let mut out = HashMap::new();
    if !bound_changes.is_empty() {
        out.insert(
            "bound_changes".to_string(),
            Message::object(EncodableValue::from(json!(bound_changes))),
        );
    }
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "backend": backend.backend_name(),
            "phase": phase,
            "polled": poll_count > 0,
            "synced": sync_count > 0,
            "boundEntities": bound_entities.len(),
            "boundChanges": bound_changes.len(),
        }))),
    );
    Ok(out)
}
