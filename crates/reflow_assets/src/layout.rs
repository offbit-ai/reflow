//! Layout backend trait — abstracts DOM/layout tree for AssetDB integration.
//!
//! The layout tree is the source of truth at initialization. AssetDB observes
//! and drives it — not the other way around.
//!
//! ## Flow
//!
//! ```text
//! DOM/LayoutTree ──hydrate──→ AssetDB ──systems──→ AssetDB ──sync──→ DOM/LayoutTree
//!                                                     ↑
//!                            @layout() queries ───────┘ (inline, no AssetDB write)
//! ```
//!
//! ## Backends
//!
//! - `DomLayoutBackend` (wasm) — browser DOM via web-sys
//! - `HeadlessLayoutBackend` (native/testing) — in-memory layout tree
//!
//! ## Query convention
//!
//! Behavior vars use `@layout(entity:property)` to query computed values inline:
//!
//! ```json
//! { "scroll": "@layout(container:scrollProgress)" }
//! ```

use anyhow::Result;
use std::sync::{Arc, Mutex};

use super::AssetDB;

/// Pluggable layout backend.
pub trait LayoutBackend: Send + Sync {
    /// Scrape existing DOM/layout tree into AssetDB as entities.
    /// Called once at startup. Creates `:dom`, `:style`, `:transform` components.
    fn hydrate(&self, db: &Arc<AssetDB>) -> Result<()>;

    /// Push changed AssetDB components back to the DOM/layout tree.
    /// Called each tick after systems have run.
    fn sync(&self, db: &Arc<AssetDB>) -> Result<()>;

    /// Read DOM/layout events and write them as `:triggers` components.
    /// Called each tick before systems run.
    fn poll_events(&self, db: &Arc<AssetDB>) -> Result<()>;

    /// Query a computed layout value. Read-only, never writes to AssetDB.
    /// Returns None if the entity or property doesn't exist.
    ///
    /// Standard properties:
    ///   `x`, `y`, `width`, `height`     — bounding box
    ///   `scrollX`, `scrollY`            — scroll offset
    ///   `scrollProgress`                — normalized scroll 0..1
    ///   `inViewport`                    — 1.0 if visible, 0.0 if not
    ///   `opacity`, `fontSize`, etc.     — computed CSS values
    ///   `parentWidth`, `parentHeight`   — parent container dimensions
    ///   `viewportWidth`, `viewportHeight` — viewport dimensions
    fn query(&self, entity: &str, property: &str) -> Option<f64>;

    /// Query a string property (tag name, text content, class list).
    fn query_string(&self, entity: &str, property: &str) -> Option<String>;

    /// Hit test: which entity is at this screen coordinate?
    fn hit_test(&self, x: f64, y: f64) -> Option<String>;

    /// Get parent entity ID.
    fn parent_of(&self, entity: &str) -> Option<String>;

    /// Get child entity IDs.
    fn children_of(&self, entity: &str) -> Option<Vec<String>>;

    /// Backend name for diagnostics.
    fn backend_name(&self) -> &'static str;
}

/// Headless layout backend — in-memory, no real DOM.
/// Stores layout values as a flat HashMap. Good for testing,
/// native rendering, server-side layout computation.
pub struct HeadlessLayoutBackend {
    nodes: std::sync::RwLock<std::collections::HashMap<String, LayoutNode>>,
}

/// A layout node in the headless backend.
#[derive(Default, Clone)]
pub struct LayoutNode {
    pub tag: String,
    pub text: String,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub scroll_height: f64,
    pub opacity: f64,
    pub props: std::collections::HashMap<String, f64>,
    pub string_props: std::collections::HashMap<String, String>,
}

impl Default for HeadlessLayoutBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadlessLayoutBackend {
    pub fn new() -> Self {
        Self {
            nodes: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Manually add/update a node (for testing or native layout engines).
    pub fn set_node(&self, entity: &str, node: LayoutNode) {
        if let Ok(mut nodes) = self.nodes.write() {
            nodes.insert(entity.to_string(), node);
        }
    }

    /// Remove a node.
    pub fn remove_node(&self, entity: &str) {
        if let Ok(mut nodes) = self.nodes.write() {
            nodes.remove(entity);
        }
    }
}

impl LayoutBackend for HeadlessLayoutBackend {
    fn hydrate(&self, db: &Arc<AssetDB>) -> Result<()> {
        // In headless mode, AssetDB is the source of truth.
        // Read :dom components and create layout nodes.
        let dom_entities = db.entities_with(&["dom"])?;
        let mut nodes = self.nodes.write().map_err(|e| anyhow::anyhow!("{}", e))?;

        for entity in &dom_entities {
            if let Ok(asset) = db.get_component(entity, "dom") {
                let v: serde_json::Value = if let Some(ref inline) = asset.entry.inline_data {
                    inline.clone()
                } else {
                    serde_json::from_slice(&asset.data).unwrap_or_default()
                };

                let mut node = LayoutNode {
                    tag: v
                        .get("tag")
                        .and_then(|v| v.as_str())
                        .unwrap_or("div")
                        .to_string(),
                    text: v
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    parent: v
                        .get("parent")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    width: v.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    height: v.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    opacity: 1.0,
                    ..Default::default()
                };

                // Read transform component for position
                if let Ok(tf) = db.get_component(entity, "transform") {
                    let tv: serde_json::Value = if let Some(ref inline) = tf.entry.inline_data {
                        inline.clone()
                    } else {
                        serde_json::from_slice(&tf.data).unwrap_or_default()
                    };
                    if let Some(pos) = tv.get("position").and_then(|v| v.as_array()) {
                        node.x = pos.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                        node.y = pos.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    }
                }

                nodes.insert(entity.clone(), node);
            }
        }
        Ok(())
    }

    fn sync(&self, db: &Arc<AssetDB>) -> Result<()> {
        // Read :style components and update layout nodes
        let nodes_to_sync: Vec<String> = {
            let nodes = self.nodes.read().map_err(|e| anyhow::anyhow!("{}", e))?;
            nodes.keys().cloned().collect()
        };

        let mut nodes = self.nodes.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        for entity in &nodes_to_sync {
            // Sync transform
            if let Ok(tf) = db.get_component(entity, "transform") {
                let tv: serde_json::Value = if let Some(ref inline) = tf.entry.inline_data {
                    inline.clone()
                } else {
                    serde_json::from_slice(&tf.data).unwrap_or_default()
                };
                if let Some(node) = nodes.get_mut(entity) {
                    if let Some(pos) = tv.get("position").and_then(|v| v.as_array()) {
                        node.x = pos.first().and_then(|v| v.as_f64()).unwrap_or(node.x);
                        node.y = pos.get(1).and_then(|v| v.as_f64()).unwrap_or(node.y);
                    }
                }
            }
            // Sync style opacity
            if let Ok(style) = db.get_component(entity, "style") {
                let sv: serde_json::Value = if let Some(ref inline) = style.entry.inline_data {
                    inline.clone()
                } else {
                    serde_json::from_slice(&style.data).unwrap_or_default()
                };
                if let Some(node) = nodes.get_mut(entity) {
                    if let Some(o) = sv.get("opacity").and_then(|v| v.as_f64()) {
                        node.opacity = o;
                    }
                }
            }
        }
        Ok(())
    }

    fn poll_events(&self, _db: &Arc<AssetDB>) -> Result<()> {
        // Headless: no events to poll. Tests inject triggers directly.
        Ok(())
    }

    fn query(&self, entity: &str, property: &str) -> Option<f64> {
        let nodes = self.nodes.read().ok()?;
        let node = nodes.get(entity)?;

        match property {
            "x" => Some(node.x),
            "y" => Some(node.y),
            "width" => Some(node.width),
            "height" => Some(node.height),
            "scrollX" => Some(node.scroll_x),
            "scrollY" => Some(node.scroll_y),
            "scrollProgress" => {
                if node.scroll_height > node.height {
                    Some(node.scroll_y / (node.scroll_height - node.height))
                } else {
                    Some(0.0)
                }
            }
            "inViewport" => Some(1.0), // headless: always visible
            "opacity" => Some(node.opacity),
            _ => node.props.get(property).copied(),
        }
    }

    fn query_string(&self, entity: &str, property: &str) -> Option<String> {
        let nodes = self.nodes.read().ok()?;
        let node = nodes.get(entity)?;

        match property {
            "tag" => Some(node.tag.clone()),
            "text" => Some(node.text.clone()),
            _ => node.string_props.get(property).cloned(),
        }
    }

    fn hit_test(&self, x: f64, y: f64) -> Option<String> {
        let nodes = self.nodes.read().ok()?;
        // Simple AABB hit test, last match wins (top of stack)
        let mut hit = None;
        for (entity, node) in nodes.iter() {
            if x >= node.x && x <= node.x + node.width && y >= node.y && y <= node.y + node.height {
                hit = Some(entity.clone());
            }
        }
        hit
    }

    fn parent_of(&self, entity: &str) -> Option<String> {
        let nodes = self.nodes.read().ok()?;
        nodes.get(entity)?.parent.clone()
    }

    fn children_of(&self, entity: &str) -> Option<Vec<String>> {
        let nodes = self.nodes.read().ok()?;
        Some(nodes.get(entity)?.children.clone())
    }

    fn backend_name(&self) -> &'static str {
        "headless"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Global layout backend registry
// ═══════════════════════════════════════════════════════════════════════════

static LAYOUT_REGISTRY: std::sync::OnceLock<
    Mutex<std::collections::HashMap<String, Arc<dyn LayoutBackend>>>,
> = std::sync::OnceLock::new();

fn layout_registry() -> &'static Mutex<std::collections::HashMap<String, Arc<dyn LayoutBackend>>> {
    LAYOUT_REGISTRY.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Register a layout backend for a given db path.
pub fn set_layout_backend(db_path: &str, backend: Arc<dyn LayoutBackend>) {
    if let Ok(mut reg) = layout_registry().lock() {
        reg.insert(db_path.to_string(), backend);
    }
}

/// Get the layout backend for a given db path.
pub fn get_layout_backend(db_path: &str) -> Option<Arc<dyn LayoutBackend>> {
    layout_registry().lock().ok()?.get(db_path).cloned()
}

/// Resolve a `@layout(entity:property)` reference.
/// Returns None if no layout backend is registered or the query fails.
pub fn resolve_layout_query(db_path: &str, query: &str) -> Option<f64> {
    let backend = get_layout_backend(db_path)?;

    // Parse "entity:property"
    let colon = query.rfind(':')?;
    let entity = &query[..colon];
    let property = &query[colon + 1..];

    backend.query(entity, property)
}
