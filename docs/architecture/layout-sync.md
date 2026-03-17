# Layout Sync — DOM / Layout Tree Integration

The layout sync system bridges AssetDB with the DOM (browser) or a native layout tree. The layout tree is the source of truth at initialization — AssetDB observes and drives it.

## Flow

```
Startup:
  DOM / Layout Tree ──hydrate()──→ AssetDB

Each tick:
  Layout ──poll_events()──→ :triggers components
                               ↓
                        Systems process (behavior, tween, state machine)
                               ↓
  AssetDB ──sync()────────→ Layout (DOM updates)

Inline queries:
  @layout(entity:property) ──→ resolved directly from layout backend
                                (never writes to AssetDB)
```

## LayoutBackend Trait

Pluggable backend for different platforms:

```rust
pub trait LayoutBackend: Send + Sync {
    fn hydrate(&self, db: &Arc<AssetDB>) -> Result<()>;
    fn sync(&self, db: &Arc<AssetDB>) -> Result<()>;
    fn poll_events(&self, db: &Arc<AssetDB>) -> Result<()>;
    fn query(&self, entity: &str, property: &str) -> Option<f64>;
    fn query_string(&self, entity: &str, property: &str) -> Option<String>;
    fn hit_test(&self, x: f64, y: f64) -> Option<String>;
    fn parent_of(&self, entity: &str) -> Option<String>;
    fn children_of(&self, entity: &str) -> Option<Vec<String>>;
}
```

| Backend | Target | Description |
|---------|--------|-------------|
| `HeadlessLayoutBackend` | Native / Testing | In-memory layout nodes |
| `DomLayoutBackend` | Wasm / Browser | web-sys DOM access |

### Queryable Properties

| Property | Type | Description |
|----------|------|-------------|
| `x`, `y` | f64 | Bounding box position |
| `width`, `height` | f64 | Bounding box dimensions |
| `scrollX`, `scrollY` | f64 | Scroll offset |
| `scrollProgress` | f64 | Normalized scroll 0..1 |
| `inViewport` | f64 | 1.0 if visible, 0.0 if not |
| `opacity` | f64 | Computed opacity |
| `parentWidth`, `parentHeight` | f64 | Parent dimensions |
| `viewportWidth`, `viewportHeight` | f64 | Viewport dimensions |
| `tag` | String | Element tag name |
| `text` | String | Text content |

## @layout() Variables

Behavior rules and expressions can reference layout-computed values inline without writing to AssetDB:

```json
{
  "entity": "hero",
  "component": "behavior",
  "data": {
    "rules": [{
      "name": "parallax",
      "target": "transform.position.y",
      "expr": "scroll * -200",
      "vars": {
        "scroll": "@layout(page:scrollProgress)"
      }
    }]
  }
}
```

The `@layout(entity:property)` prefix tells the variable resolver to query the layout backend directly. The value is resolved at evaluation time — ephemeral, never stored.

## Two-Way Binding

Entities with a `:bind` component get automatic bi-directional sync. No explicit DAG wiring needed for the data flow itself.

### Full Bind

```json
{ "entity": "slider", "component": "bind", "data": true }
```

Syncs all standard properties: transform, style, value, scroll.

### Selective Bind

```json
{
  "entity": "input_field",
  "component": "bind",
  "data": {
    "value": true,
    "scroll": true,
    "transform": false,
    "style": false
  }
}
```

Only syncs the properties you opt into.

### What Happens Each Tick

For bound entities, the LayoutSyncSystem automatically:

| Direction | What | When |
|-----------|------|------|
| **Pull** | Layout position/scroll/value → AssetDB | Before systems run |
| **Push** | AssetDB transform/style changes → Layout | After systems run |

### Traceability

All bind operations include `metadata.source = "layout_pull"` so you can trace them in Reflow's tracing system. The `bound_changes` outport emits every pull/push operation for the DAG inspector:

```json
[
  {"entity": "slider", "direction": "pull", "property": "transform"},
  {"entity": "slider", "direction": "push"}
]
```

### Bind vs Explicit Wiring

| Approach | When to use |
|----------|-------------|
| `:bind` component | Standard form inputs, draggable elements, scroll-driven animations. Set it and forget it. |
| Explicit DAG wiring | Complex logic between layout and state. Custom pull/push conditions. Multi-step transformations. |

Both are visible in the DAG inspector. Bind is convenience — it doesn't bypass the system, it just saves you from wiring the obvious.

## DOM Component Schemas

### `:dom` — Element definition

```json
{
  "tag": "button",
  "text": "Click me",
  "parent": "nav",
  "width": 120,
  "height": 40
}
```

### `:style` — Visual properties

```json
{
  "opacity": 1.0,
  "backgroundColor": "#007bff",
  "borderRadius": "8px",
  "transform": "scale(1.0)"
}
```

### `:transform` — Position/rotation/scale

```json
{
  "position": [100, 200, 0],
  "rotation": [0, 0, 0],
  "scale": [1, 1, 1]
}
```

### `:triggers` — Events (consumed each tick)

```json
["pointerEnter", "pointerDown", "scroll"]
```

Written by `poll_events()` or external input systems. Read and cleared by StateMachineSystem / BehaviorSystem.

### `:bind` — Two-way sync toggle

```json
true
```
or selective:
```json
{ "transform": true, "value": true, "scroll": false }
```

## Example: Animated Button

Set up an interactive button with hover/press states and tween animations, all driven by data:

```rust
let db = get_or_create_db("./app.db")?;

// Element
db.set_component_json("btn", "dom", json!({
    "tag": "button", "text": "Submit"
}), json!({}))?;

db.set_component_json("btn", "transform", json!({
    "position": [0, 0, 0], "scale": [1, 1, 1]
}), json!({}))?;

db.set_component_json("btn", "style", json!({
    "opacity": 1.0, "backgroundColor": "#007bff"
}), json!({}))?;

// State machine
db.put_json("btn:state_machine", json!({
    "current": "idle",
    "states": {
        "idle": { "onEnter": { "tween": "btn_scale_normal" } },
        "hover": { "onEnter": { "tween": "btn_scale_up" } },
        "pressed": { "onEnter": { "tween": "btn_scale_down" } }
    },
    "transitions": [
        { "from": "idle", "to": "hover", "trigger": "pointerEnter" },
        { "from": "hover", "to": "idle", "trigger": "pointerLeave" },
        { "from": "hover", "to": "pressed", "trigger": "pointerDown" },
        { "from": "pressed", "to": "hover", "trigger": "pointerUp" }
    ]
}), json!({}))?;

// Tweens
db.put_json("btn_scale_up:tween", json!({
    "target": "btn:transform.scale",
    "from": [1, 1, 1], "to": [1.05, 1.05, 1.05],
    "duration": 0.15, "easing": "easeOutCubic",
    "state": "paused"
}), json!({}))?;

db.put_json("btn_scale_down:tween", json!({
    "target": "btn:transform.scale",
    "from": [1.05, 1.05, 1.05], "to": [0.95, 0.95, 0.95],
    "duration": 0.1, "easing": "easeOutCubic",
    "state": "paused"
}), json!({}))?;

db.put_json("btn_scale_normal:tween", json!({
    "target": "btn:transform.scale",
    "from": [0.95, 0.95, 0.95], "to": [1, 1, 1],
    "duration": 0.2, "easing": "easeOutCubic",
    "state": "paused"
}), json!({}))?;

// Bind for auto sync
db.set_component_json("btn", "bind", json!(true), json!({}))?;
```

DAG:
```
IntervalTrigger(16ms) → LayoutSync(phase: "both", entity: "btn")
                      → StateMachineSystem(entity: "btn")
                      → TweenSystem(entity: "btn")
```

The button responds to hover/press with smooth scale animations. No Rust code for the interaction logic — it's all data in the AssetDB.
