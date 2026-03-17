# Game Programming with Reflow

Reflow's game architecture follows the **Entity-Component-System** pattern. The AssetDB is the world. Components are queryable data. DAG actors are systems.

```
AssetDB (World)          Reflow DAG (Systems)         External Tools
┌──────────────┐        ┌──────────────────┐        ┌──────────────┐
│ player:      │◀──────▶│ PhysicsSystem    │        │ Zeal Editor  │
│   transform  │        │ CameraSystem     │        │ Debug Tools  │
│   rigidbody  │        │ LightCollector   │        │ Scripts      │
│   collider   │        │ MaterialSystem   │        │ Unit Tests   │
│   mesh       │        │ RenderSystem     │        │              │
│   material   │        └──────────────────┘        └──────────────┘
│              │                                           │
│ sun:light    │◀──────────────────────────────────────────┘
│ main:camera  │         any tool reads/writes the same DB
└──────────────┘
```

## Quick Start

### 1. Set up the world

Create entities by putting components into the AssetDB. An entity is just a name prefix. A component is a type suffix.

```rust
use reflow_assets::get_or_create_db;
use serde_json::json;

let db = get_or_create_db("./game.db")?;

// Player entity
db.set_component_json("player", "transform", json!({
    "position": [0.0, 1.0, 0.0],
    "rotation": [0.0, 0.0, 0.0, 1.0],
    "scale": [1.0, 1.0, 1.0],
}), json!({}))?;

db.set_component_json("player", "rigidbody", json!({
    "bodyType": "dynamic",
    "mass": 80.0,
    "linearDamping": 0.1,
    "gravityScale": 1.0,
}), json!({}))?;

db.set_component_json("player", "collider", json!({
    "shape": "capsule",
    "radius": 0.3,
    "height": 1.8,
    "friction": 0.5,
    "restitution": 0.1,
}), json!({}))?;

// Ground
db.set_component_json("ground", "transform", json!({
    "position": [0.0, 0.0, 0.0],
}), json!({}))?;

db.set_component_json("ground", "rigidbody", json!({
    "bodyType": "static",
}), json!({}))?;

db.set_component_json("ground", "collider", json!({
    "shape": "box",
    "halfExtents": [50.0, 0.1, 50.0],
}), json!({}))?;

// Camera
db.set_component_json("main", "camera", json!({
    "mode": "thirdPerson",
    "target": "player",
    "fov": 60.0,
    "distance": 5.0,
    "height": 2.0,
    "orbitPitch": 0.3,
    "active": true,
}), json!({}))?;

// Sun light
db.set_component_json("sun", "light", json!({
    "type": "directional",
    "direction": [0.0, -1.0, 0.5],
    "color": [1.0, 1.0, 0.9],
    "intensity": 2.0,
    "castShadow": true,
}), json!({}))?;

// Torch (point light)
db.set_component_json("torch", "transform", json!({
    "position": [3.0, 2.0, 1.0],
}), json!({}))?;

db.set_component_json("torch", "light", json!({
    "type": "point",
    "color": [1.0, 0.6, 0.2],
    "range": 10.0,
    "intensity": 3.0,
}), json!({}))?;

// Material
db.set_component_json("player", "material", json!({
    "albedo": [0.8, 0.2, 0.1],
    "metallic": 0.0,
    "roughness": 0.5,
}), json!({}))?;
```

### 2. Wire the game loop DAG

The DAG connects systems. Each system reads components, processes, writes results back.

```rust
use reflow_network::{network::{Network, NetworkConfig}, message::Message};

let mut net = Network::new(NetworkConfig::default());

// Register system actors
for tpl in [
    "tpl_interval_trigger",
    "tpl_scene_physics",
    "tpl_scene_camera",
    "tpl_scene_light_collector",
    "tpl_scene_material",
] {
    net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
}

// Game tick at 60fps
net.add_node("tick", "tpl_interval_trigger", config(json!({
    "interval": 16, "startImmediately": true,
})))?;

// Systems — all read/write the same AssetDB
net.add_node("physics", "tpl_scene_physics", config(json!({
    "$db": "./game.db",
    "gravity": [0.0, -9.81, 0.0],
    "dt": 0.016,
})))?;

net.add_node("camera", "tpl_scene_camera", config(json!({
    "$db": "./game.db",
    "aspect": 1.777,
    "cameraTag": "main",
})))?;

net.add_node("lights", "tpl_scene_light_collector", config(json!({
    "$db": "./game.db",
})))?;

net.add_node("materials", "tpl_scene_material", config(json!({
    "$db": "./game.db",
})))?;

// Wire: tick drives all systems
net.add_connection(wire("tick", "trigger", "physics", "tick"));
net.add_connection(wire("tick", "trigger", "camera", "tick"));
net.add_connection(wire("tick", "trigger", "lights", "tick"));
net.add_connection(wire("tick", "trigger", "materials", "tick"));

// Start
net.add_initial(iip("tick", "_trigger", Message::Flow));
net.start()?;
```

### 3. Query the world from anywhere

The AssetDB is the single source of truth. Any tool can read and write it — not just the DAG.

```rust
// Inspect an entity
let snapshot = db.entity_snapshot("player")?;
println!("{}", serde_json::to_string_pretty(&snapshot)?);
// {
//   "transform": { "position": [0.0, 0.83, 0.0], ... },
//   "rigidbody": { "bodyType": "dynamic", "mass": 80.0, ... },
//   "collider": { "shape": "capsule", ... },
//   "velocity": { "linear": [0.0, -0.12, 0.0], ... }
// }

// Find all dynamic bodies
let dynamic_entities = db.query_dsl(&json!({
    "type": "rigidbody",
    "metadata.bodyType": "dynamic",
}))?;

// Find all entities with both mesh and material
let renderable = db.entities_with(&["mesh", "material", "transform"])?;

// Teleport the player
db.set_component_json("player", "transform", json!({
    "position": [10.0, 5.0, 0.0],
    "rotation": [0.0, 0.0, 0.0, 1.0],
    "scale": [1.0, 1.0, 1.0],
}), json!({}))?;
```

## Component Reference

### transform
Position, rotation, and scale of an entity in the world.
```json
{
    "position": [0.0, 0.0, 0.0],
    "rotation": [0.0, 0.0, 0.0, 1.0],
    "scale": [1.0, 1.0, 1.0]
}
```

### rigidbody
Physics body properties. The physics system picks up any entity with both `rigidbody` and `transform`.
```json
{
    "bodyType": "dynamic",
    "mass": 1.0,
    "linearDamping": 0.1,
    "angularDamping": 0.1,
    "gravityScale": 1.0,
    "ccd": false
}
```
Body types: `"dynamic"` (simulated), `"static"` (immovable), `"kinematic"` (user-driven).

### collider
Collision shape attached to a rigidbody.
```json
{
    "shape": "capsule",
    "radius": 0.3,
    "height": 1.8,
    "friction": 0.5,
    "restitution": 0.3,
    "isSensor": false
}
```
Shapes: `"box"` (halfExtents), `"sphere"` (radius), `"capsule"` (radius + height), `"cylinder"` (radius + height).

### camera
View configuration. Multiple cameras per scene are supported. Use `"active": true` or pass a tag to select which renders.
```json
{
    "mode": "thirdPerson",
    "target": "player",
    "fov": 60.0,
    "near": 0.1,
    "far": 1000.0,
    "distance": 5.0,
    "height": 2.0,
    "orbitYaw": 0.0,
    "orbitPitch": 0.3,
    "active": true
}
```
Modes: `"fixed"` (position + target), `"firstPerson"` (attached to entity), `"thirdPerson"` (follow with offset), `"orbit"` (rotate around center).

### light
Light source. Position comes from the entity's `transform` component for point/spot lights.
```json
{
    "type": "directional",
    "direction": [0.0, -1.0, 0.5],
    "color": [1.0, 1.0, 0.9],
    "intensity": 2.0,
    "castShadow": true,
    "range": 20.0,
    "innerAngle": 30.0,
    "outerAngle": 45.0
}
```
Types: `"directional"` (sun), `"point"` (bulb), `"spot"` (cone), `"ambient"` (fill).

### material
PBR material properties. Texture fields reference AssetDB entity IDs.
```json
{
    "albedo": [0.8, 0.2, 0.1],
    "metallic": 0.0,
    "roughness": 0.5,
    "emissive": [0.0, 0.0, 0.0],
    "emissiveStrength": 0.0,
    "ao": 1.0,
    "alphaMode": "opaque",
    "doubleSided": false,
    "albedoTexture": "wood:texture",
    "normalTexture": "wood_normal:texture"
}
```

## System Actors

| Actor | Template ID | Reads | Writes | Outputs |
|-------|-------------|-------|--------|---------|
| ScenePhysicsSystem | `tpl_scene_physics` | rigidbody, collider, transform | transform, velocity | collision pairs |
| SceneCameraSystem | `tpl_scene_camera` | camera, transform (of target) | camera_matrices | active camera data |
| SceneLightCollector | `tpl_scene_light_collector` | light, transform | — | packed light buffer |
| SceneMaterialSystem | `tpl_scene_material` | material | — | packed material buffer |

## Spawning Entities

Use `spawn_from` to instantiate prefabs:

```rust
// Define a template once
db.set_component_json("crate_template", "transform", json!({
    "position": [0.0, 0.0, 0.0],
}), json!({}))?;
db.set_component_json("crate_template", "rigidbody", json!({
    "bodyType": "dynamic", "mass": 10.0,
}), json!({}))?;
db.set_component_json("crate_template", "collider", json!({
    "shape": "box", "halfExtents": [0.5, 0.5, 0.5],
}), json!({}))?;
db.set_component_json("crate_template", "material", json!({
    "albedo": [0.6, 0.4, 0.2], "roughness": 0.8,
}), json!({}))?;

// Spawn 10 crates at different positions
for i in 0..10 {
    let name = format!("crate_{}", i);
    db.spawn_from("crate_template", &name)?;
    db.set_component_json(&name, "transform", json!({
        "position": [i as f64 * 2.0, 5.0, 0.0],
    }), json!({}))?;
}
```

The physics system picks them up automatically on the next tick.

## Importing Assets

Load a Mixamo character into the world:

```rust
// Load file
let glb_data = std::fs::read("character.glb")?;

// Import extracts mesh, skeleton, animation, skin
// Wire in DAG: FileLoad → GltfImport → AssetStore
// Or programmatically:
db.put("character:mesh", &mesh_bytes, json!({"stride": 24}))?;
db.put_json("character:skeleton", skeleton_json, json!({}))?;
db.put_json("character:animation", clip_json, json!({}))?;

// Create entity using imported assets
db.set_component_json("npc", "transform", json!({
    "position": [5.0, 0.0, 3.0],
}), json!({}))?;
db.set_component_json("npc", "rigidbody", json!({
    "bodyType": "dynamic", "mass": 60.0,
}), json!({}))?;
db.set_component_json("npc", "collider", json!({
    "shape": "capsule", "radius": 0.3, "height": 1.6,
}), json!({}))?;
```

## Multiple Cameras

Switch cameras by tag:

```rust
// Define cameras
db.set_component_json("gameplay_cam", "camera", json!({
    "mode": "thirdPerson", "target": "player", "fov": 60, "active": true,
}), json!({}))?;
db.tag("gameplay_cam:camera", &["gameplay"])?;

db.set_component_json("cutscene_cam", "camera", json!({
    "mode": "fixed", "position": [10, 5, 0], "target": [0, 0, 0], "active": false,
}), json!({}))?;
db.tag("cutscene_cam:camera", &["cutscene"])?;

// In the DAG, the camera system accepts a tag input:
// wire("camera_selector", "tag", "camera", "camera_tag")
// Send "cutscene" to switch to the cutscene camera
```

## Collision Handling

The physics system outputs collision pairs. Wire a handler in the DAG:

```
tick → ScenePhysicsSystem
           │
           └→ collisions → YourCollisionHandler (user actor)
                               │
                               └→ reads collision pairs:
                                  [{ "a": "player", "b": "coin_3" }]
                                  → removes coin, adds score
```

## Architecture Summary

| Concern | Where it lives | Why |
|---------|---------------|-----|
| Entity data | AssetDB components | Queryable by any tool, not coupled to DAG |
| Game logic | DAG actors (systems) | Visual wiring, reorderable, hot-swappable |
| Execution order | DAG connections | Explicit, debuggable dataflow |
| Persistence | AssetDB storage backend | File, IndexedDB, or S3 — same API |
| Editor integration | Direct AssetDB reads/writes | No DAG needed for inspection/editing |
