//! Physics system — rapier3d-backed rigid body simulation.
//!
//! Reads rigidbody + collider + transform components from AssetDB,
//! steps the physics simulation, writes back updated transforms.
//!
//! ## Component schemas
//!
//! ### `entity:rigidbody`
//! ```json
//! {
//!   "bodyType": "dynamic",
//!   "mass": 1.0,
//!   "linearDamping": 0.1,
//!   "angularDamping": 0.1,
//!   "gravityScale": 1.0,
//!   "ccd": false,
//!   "lockRotationX": false,
//!   "lockRotationY": false,
//!   "lockRotationZ": false
//! }
//! ```
//! bodyType: "dynamic", "kinematic", "static"
//!
//! ### `entity:collider`
//! ```json
//! {
//!   "shape": "capsule",
//!   "radius": 0.3,
//!   "height": 1.8,
//!   "halfExtents": [0.5, 0.5, 0.5],
//!   "friction": 0.5,
//!   "restitution": 0.3,
//!   "isSensor": false,
//!   "collisionGroup": 1,
//!   "collisionMask": 4294967295
//! }
//! ```
//! Shapes: "box", "sphere", "capsule", "cylinder"
//!
//! ### `entity:transform`
//! ```json
//! {
//!   "position": [0.0, 5.0, 0.0],
//!   "rotation": [0.0, 0.0, 0.0, 1.0],
//!   "scale": [1.0, 1.0, 1.0]
//! }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rapier3d::prelude::*;
use rapier3d::na::{UnitQuaternion, Quaternion};

/// Persistent physics world state shared across ticks.
struct PhysicsWorld {
    gravity: Vector<f32>,
    integration_params: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    /// Maps entity name → rapier rigid body handle
    entity_to_body: HashMap<String, RigidBodyHandle>,
    /// Maps entity name → rapier collider handle
    entity_to_collider: HashMap<String, ColliderHandle>,
}

impl PhysicsWorld {
    fn new(gravity: [f32; 3]) -> Self {
        Self {
            gravity: vector![gravity[0], gravity[1], gravity[2]],
            integration_params: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            entity_to_body: HashMap::new(),
            entity_to_collider: HashMap::new(),
        }
    }

    fn step(&mut self, dt: f32) {
        self.integration_params.dt = dt;
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_params,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );
    }
}

/// Global physics worlds keyed by db path.
static PHYSICS_WORLDS: std::sync::OnceLock<Mutex<HashMap<String, Arc<Mutex<PhysicsWorld>>>>> =
    std::sync::OnceLock::new();

fn get_physics_world(db_path: &str, gravity: [f32; 3]) -> Arc<Mutex<PhysicsWorld>> {
    let registry = PHYSICS_WORLDS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut reg = registry.lock().unwrap();
    reg.entry(db_path.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(PhysicsWorld::new(gravity))))
        .clone()
}

#[actor(
    ScenePhysicsSystemActor,
    inports::<10>(tick, entity_id),
    outports::<1>(collisions, metadata, error),
    state(MemoryState)
)]
pub async fn physics_system_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let gravity = config
        .get("gravity")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(-9.81) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            ]
        })
        .unwrap_or([0.0, -9.81, 0.0]);
    let dt = config
        .get("dt")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0 / 60.0) as f32;

    let db = get_or_create_db(db_path)?;
    let world = get_physics_world(db_path, gravity);
    let mut world = world.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Resolve which entities to simulate
    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let physics_entities = if selected.is_empty() {
        // Fallback: if nothing explicitly selected, find entities with rigidbody+transform
        db.entities_with(&["rigidbody", "transform"])?
    } else {
        // Filter selected to only those that actually have rigidbody+transform
        selected.into_iter().filter(|e| {
            db.has_component(e, "rigidbody") && db.has_component(e, "transform")
        }).collect()
    };

    for entity in &physics_entities {
        let rb_asset = db.get_component(entity, "rigidbody")?;
        let tf_asset = db.get_component(entity, "transform")?;

        let rb: Value = if let Some(ref inline) = rb_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&rb_asset.data).unwrap_or(json!({}))
        };
        let tf: Value = if let Some(ref inline) = tf_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&tf_asset.data).unwrap_or(json!({}))
        };

        let pos = read_vec3(&tf, "position", [0.0, 0.0, 0.0]);
        let rot = read_vec4(&tf, "rotation", [0.0, 0.0, 0.0, 1.0]);

        if world.entity_to_body.contains_key(entity.as_str()) {
            // Entity already in physics world — update kinematic targets if needed
            let body_type = rb.get("bodyType").and_then(|v| v.as_str()).unwrap_or("dynamic");
            if body_type == "kinematic" {
                if let Some(&handle) = world.entity_to_body.get(entity.as_str()) {
                    if let Some(body) = world.rigid_body_set.get_mut(handle) {
                        body.set_next_kinematic_position(Isometry::from_parts(
                            Translation::new(pos[0], pos[1], pos[2]),
                            UnitQuaternion::new_normalize(
                                Quaternion::new(rot[3], rot[0], rot[1], rot[2]),
                            ),
                        ));
                    }
                }
            }
            continue;
        }

        // New entity — create rigid body
        let body_type = rb.get("bodyType").and_then(|v| v.as_str()).unwrap_or("dynamic");
        let mass = rb.get("mass").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let lin_damp = rb.get("linearDamping").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
        let ang_damp = rb.get("angularDamping").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
        let grav_scale = rb.get("gravityScale").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let ccd = rb.get("ccd").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut builder = match body_type {
            "static" => RigidBodyBuilder::fixed(),
            "kinematic" => RigidBodyBuilder::kinematic_position_based(),
            _ => RigidBodyBuilder::dynamic(),
        };

        builder = builder
            .translation(vector![pos[0], pos[1], pos[2]])
            .rotation(vector![rot[0], rot[1], rot[2]]) // axis-angle approximation
            .linear_damping(lin_damp)
            .angular_damping(ang_damp)
            .gravity_scale(grav_scale)
            .ccd_enabled(ccd)
            .additional_mass(mass);

        let body_handle = world.rigid_body_set.insert(builder.build());
        world
            .entity_to_body
            .insert(entity.clone(), body_handle);

        // Attach collider if entity has one
        if let Ok(col_asset) = db.get_component(entity, "collider") {
            let col: Value = if let Some(ref inline) = col_asset.entry.inline_data {
                inline.clone()
            } else {
                serde_json::from_slice(&col_asset.data).unwrap_or(json!({}))
            };

            let shape = col.get("shape").and_then(|v| v.as_str()).unwrap_or("box");
            let friction = col.get("friction").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
            let restitution = col.get("restitution").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
            let is_sensor = col.get("isSensor").and_then(|v| v.as_bool()).unwrap_or(false);

            let collider_shape: SharedShape = match shape {
                "sphere" => {
                    let radius = col.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    SharedShape::ball(radius)
                }
                "capsule" => {
                    let radius = col.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
                    let height = col.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    SharedShape::capsule_y(height / 2.0, radius)
                }
                "cylinder" => {
                    let radius = col.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    let height = col.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    SharedShape::cylinder(height / 2.0, radius)
                }
                _ => {
                    // Box
                    let he = read_vec3(&col, "halfExtents", [0.5, 0.5, 0.5]);
                    SharedShape::cuboid(he[0], he[1], he[2])
                }
            };

            let collider = ColliderBuilder::new(collider_shape)
                .friction(friction)
                .restitution(restitution)
                .sensor(is_sensor)
                .build();

            // Split borrow: destructure world to borrow collider_set and rigid_body_set simultaneously
            let PhysicsWorld { ref mut collider_set, ref mut rigid_body_set, ref mut entity_to_collider, .. } = *world;
            let collider_handle = collider_set.insert_with_parent(
                collider,
                body_handle,
                rigid_body_set,
            );
            entity_to_collider.insert(entity.clone(), collider_handle);
        }
    }

    // Step physics
    world.step(dt);

    // Write back: rapier → AssetDB transforms
    let mut updated = 0usize;
    let mut collision_pairs = Vec::new();

    for (entity, &handle) in &world.entity_to_body {
        if let Some(body) = world.rigid_body_set.get(handle) {
            if !body.is_dynamic() {
                continue;
            }
            let pos = body.translation();
            let rot = body.rotation();
            let vel = body.linvel();

            // Write updated transform back to AssetDB
            let _ = db.set_component_json(
                entity,
                "transform",
                json!({
                    "position": [pos.x, pos.y, pos.z],
                    "rotation": [rot.i, rot.j, rot.k, rot.w],
                    "scale": [1.0, 1.0, 1.0],
                }),
                json!({ "source": "physics" }),
            );

            // Update velocity in rigidbody component
            let av = body.angvel();
            let _ = db.set_component_json(
                entity,
                "velocity",
                json!({
                    "linear": [vel.x, vel.y, vel.z],
                    "angular": [av.x, av.y, av.z],
                }),
                json!({}),
            );

            updated += 1;
        }
    }

    // Collect contact pairs
    for pair in world.narrow_phase.contact_pairs() {
        let entity_a = world
            .entity_to_collider
            .iter()
            .find(|(_, &h)| h == pair.collider1)
            .map(|(e, _)| e.clone());
        let entity_b = world
            .entity_to_collider
            .iter()
            .find(|(_, &h)| h == pair.collider2)
            .map(|(e, _)| e.clone());

        if let (Some(a), Some(b)) = (entity_a, entity_b) {
            if pair.has_any_active_contact {
                collision_pairs.push(json!({ "a": a, "b": b }));
            }
        }
    }

    let mut out = HashMap::new();
    out.insert(
        "collisions".to_string(),
        Message::object(EncodableValue::from(json!(collision_pairs))),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "entitiesUpdated": updated,
            "bodyCount": world.rigid_body_set.len(),
            "colliderCount": world.collider_set.len(),
            "collisionPairs": collision_pairs.len(),
            "dt": dt,
        }))),
    );
    Ok(out)
}

fn read_vec3(v: &Value, key: &str, default: [f32; 3]) -> [f32; 3] {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(default[0] as f64) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(default[1] as f64) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(default[2] as f64) as f32,
            ]
        })
        .unwrap_or(default)
}

fn read_vec4(v: &Value, key: &str, default: [f32; 4]) -> [f32; 4] {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(default[0] as f64) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(default[1] as f64) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(default[2] as f64) as f32,
                a.get(3).and_then(|v| v.as_f64()).unwrap_or(default[3] as f64) as f32,
            ]
        })
        .unwrap_or(default)
}
