# AssetDB — Entity-Component-System Data Store

AssetDB is Reflow's persistent, queryable world state. It replaces in-memory actor state for anything that needs to be shared across systems, inspected by tools, or survive workflow restarts.

## Design Principles

1. **Entities are data, not actors.** An entity's physics, camera, material — these are JSON components in the DB, not Rust structs.
2. **Systems are DAG actors.** The DAG wires which systems run, on which entities, in what order.
3. **Any tool can query.** Zeal editor, debug inspector, Python scripts, unit tests — all read/write the same DB. Not coupled to Reflow.
4. **Explicit over magic.** The DAG shows the flow. No hidden subscriptions (except opt-in `:bind`).

## Entity-Component Model

An **entity** is a name prefix. A **component** is a type suffix. The ID convention is `entity:component`.

```
player:transform    → { "position": [0, 1, 0], "rotation": [0, 0, 0, 1] }
player:rigidbody    → { "bodyType": "dynamic", "mass": 80 }
player:collider     → { "shape": "capsule", "radius": 0.3, "height": 1.8 }
player:mesh         → <binary 24-byte stride>
player:material     → { "albedo": [0.8, 0.2, 0.1], "roughness": 0.5 }

sun:light           → { "type": "directional", "color": [1, 1, 0.9] }
main:camera         → { "mode": "thirdPerson", "target": "player", "fov": 60 }
```

## API

### Put / Get (entity-style)

```rust
let db = AssetDB::open("./game.db")?;    // FileBackend (native)
let db = AssetDB::in_memory()?;           // MemoryBackend (wasm/testing)

// Put binary data (mesh, texture)
db.put("snake:mesh", &mesh_bytes, json!({"stride": 24}))?;

// Put JSON data (transform, material, config)
db.put_json("player:transform", json!({"position": [0, 1, 0]}), json!({}))?;

// Get
let asset = db.get("snake:mesh")?;
let data: Vec<u8> = asset.data;
let meta: Value = asset.entry.metadata;

// Check existence
db.has("player:rigidbody");  // true/false
```

### Component Access (ECS-style)

```rust
// Set component on entity
db.set_component_json("player", "transform", json!({...}), json!({}))?;

// Get component
let tf = db.get_component("player", "transform")?;

// List all components on an entity
db.components_of("player")?;           // → ["transform", "rigidbody", "mesh"]

// Find entities with specific components
db.entities_with(&["rigidbody", "transform"])?;  // → ["player", "enemy_1"]

// Entity snapshot (all components as JSON)
db.entity_snapshot("player")?;
// → { "transform": {...}, "rigidbody": {...}, "material": {...} }

// Spawn from template
db.spawn_from("crate_template", "crate_42")?;

// Destroy entity (removes all components)
db.destroy_entity("crate_42")?;
```

### Tags

```rust
db.tag("sword:mesh", &["weapon", "melee"])?;

db.query_dsl(&json!({"tags": ["weapon"]}))?;              // has ANY tag
db.query_dsl(&json!({"tags": {"$all": ["weapon", "melee"]}}))?;  // has ALL tags
```

### Query DSL

Queries describe the shape of what you're looking for. The config IS the query.

```json
{ "type": "mesh", "tags": ["snake"], "$sort": "newest", "$limit": 5 }
{ "name": { "$contains": "body" }, "metadata.stride": 24 }
{ "type": { "$in": ["mesh", "texture"] }, "size": { "$gt": 10000 } }
{ "tags": { "$all": ["weapon", "melee"] } }
```

**Operators:** `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$contains`, `$startsWith`, `$all`, `$between`, `$not`, `$exists`

**Control keys:** `$sort` (newest/oldest/largest/smallest/name), `$limit`

## Storage Backends

| Backend | Target | Persistence | Use case |
|---------|--------|-------------|----------|
| `FileBackend` | Native | Directory on disk | Desktop, CI |
| `MemoryBackend` | Any | None (RAM) | Testing |
| `IndexedDbBackend` | Wasm | Browser IndexedDB | Web editor |
| S3Backend *(planned)* | Any | S3 bucket | Cloud workflows |

All backends implement the `StorageBackend` trait:

```rust
pub trait StorageBackend: Send + Sync {
    fn read_manifest(&self) -> Result<Vec<AssetEntry>>;
    fn write_manifest(&self, entries: &[AssetEntry]) -> Result<()>;
    fn read_blob(&self, hash: &str) -> Result<Vec<u8>>;
    fn write_blob(&self, hash: &str, data: &[u8]) -> Result<()>;
    fn blob_exists(&self, hash: &str) -> bool;
    fn delete_blob(&self, hash: &str) -> Result<()>;
}
```

## Compression

Binary blobs are transparently LZ4-compressed (via `lz4_flex`, pure Rust, wasm-safe). Compression is selective:

- Mesh/animation blobs: compressed (~15-25% savings)
- JSON components: compressed (~60-80% savings)
- Textures/audio/video: stored raw (already compressed)
- Small blobs (<256 bytes): stored raw

`get()` always returns uncompressed data. Callers never see compression.

## Content Addressing

Blobs are stored by content hash. Identical data → same hash → same blob. Re-importing the same asset costs zero additional storage. `put()` is an upsert — same entity ID overwrites, but if the content hasn't changed, no disk write occurs.

## System Actors

Systems read components, process, write results back. The DAG determines execution order and which entities each system operates on.

| System | Template | Reads | Writes |
|--------|----------|-------|--------|
| ScenePhysicsSystem | `tpl_scene_physics` | rigidbody, collider, transform | transform, velocity |
| SceneCameraSystem | `tpl_scene_camera` | camera | camera_matrices |
| SceneLightCollector | `tpl_scene_light_collector` | light, transform | packed GPU buffer |
| SceneMaterialSystem | `tpl_scene_material` | material | packed GPU buffer |
| TweenSystem | `tpl_tween_system` | tween | target property |
| TimelineSystem | `tpl_timeline_system` | timeline | target properties |
| StateMachineSystem | `tpl_state_machine_system` | state_machine, triggers | state_machine |
| BehaviorSystem | `tpl_behavior_system` | behavior | target properties |
| LayoutSyncSystem | `tpl_layout_sync` | dom, style, transform, bind | triggers, computed |

### Entity Selectors

Every system accepts explicit entity targeting:

```json
{ "entity": "player" }                           // single entity
{ "entities": ["sun", "torch", "lamp"] }         // explicit list
{ "selector": { "tags": ["enemy"] } }            // query-based
```

The `entity_id` inport allows dynamic selection per-tick from the DAG.
