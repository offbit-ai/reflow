//! Layout sync system — bridges AssetDB ↔ DOM/layout tree.
//!
//! Runs each tick:
//! 1. poll_events — reads DOM events → writes :triggers components
//! 2. (other systems run — behavior, tween, state machine, etc.)
//! 3. sync — pushes changed AssetDB components → DOM
//!
//! ## DAG wiring
//!
//! ```text
//! tick → LayoutSync(phase: "poll")  → BehaviorSystem → TweenSystem → ...
//!                                                                      ↓
//!                                              ... → LayoutSync(phase: "sync")
//! ```
//!
//! Or single actor with phase: "both" for simpler DAGs.
//!
//! ## First run
//!
//! On first invocation, automatically calls hydrate() to scrape the
//! existing DOM/layout tree into AssetDB.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::{get_or_create_db, layout};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static HYDRATED: AtomicBool = AtomicBool::new(false);

#[actor(
    LayoutSyncSystemActor,
    inports::<10>(tick),
    outports::<1>(metadata),
    state(MemoryState)
)]
pub async fn layout_sync_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
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
            // Auto-register headless backend if none exists
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

    if phase == "poll" || phase == "both" {
        backend.poll_events(&db)?;
        poll_count = 1;
    }

    if phase == "sync" || phase == "both" {
        backend.sync(&db)?;
        sync_count = 1;
    }

    let mut out = HashMap::new();
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "backend": backend.backend_name(),
            "phase": phase,
            "polled": poll_count > 0,
            "synced": sync_count > 0,
        }))),
    );
    Ok(out)
}
