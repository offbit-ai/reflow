//! Knowledge graph projection — projects AssetDB + Reflow Network
//! into a property graph queryable via Cypher (KyuGraph).
//!
//! ## Graph schema
//!
//! ```cypher
//! // Nodes
//! (:Entity {name, created_at})
//! (:Component {id, type, entity, data})
//! (:Actor {name, template, config})
//! (:Port {name, direction, actor})
//!
//! // Relationships
//! (Entity)-[:HAS_COMPONENT]->(Component)
//! (Actor)-[:CONNECTS_TO {from_port, to_port}]->(Actor)
//! (Actor)-[:TARGETS]->(Entity)  // via entity selector
//! (Component)-[:REFERENCES]->(Entity)  // e.g. camera.target = "player"
//! (Entity)-[:SPAWNED_FROM]->(Entity)  // template relationship
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! let projector = GraphProjection::new();
//! db.add_listener(Arc::new(projector));
//!
//! // AssetDB mutations now emit Cypher deltas:
//! // MERGE (e:Entity {name: "player"})
//! // MERGE (c:Component {id: "player:rigidbody", type: "rigidbody"})
//! // SET c.data = {mass: 80, bodyType: "dynamic"}
//! // MERGE (e)-[:HAS_COMPONENT]->(c)
//! ```
//!
//! ## AI Agent queries
//!
//! ```cypher
//! // Find entities missing colliders
//! MATCH (e:Entity)-[:HAS_COMPONENT]->(rb:Component {type: "rigidbody"})
//! WHERE NOT (e)-[:HAS_COMPONENT]->(:Component {type: "collider"})
//! RETURN e.name
//!
//! // Trace data flow to physics
//! MATCH path = (a:Actor)-[:CONNECTS_TO*]->(p:Actor {template: "tpl_scene_physics"})
//! RETURN path
//!
//! // Find all entities with behavior rules
//! MATCH (e:Entity)-[:HAS_COMPONENT]->(b:Component {type: "behavior"})
//! RETURN e.name, b.data
//! ```

use super::{Delta, DeltaListener, DeltaOp};
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};

/// Cypher statement ready for KyuGraph execution.
#[derive(Debug, Clone)]
pub struct CypherDelta {
    pub statement: String,
    pub params: Value,
}

/// Collects Cypher deltas from AssetDB mutations.
/// Feed these to KyuGraph (in-memory or S3-backed) for AI agent queries.
pub struct GraphProjection {
    /// Buffered Cypher statements. Consumers drain this.
    buffer: RwLock<Vec<CypherDelta>>,
}

impl GraphProjection {
    pub fn new() -> Self {
        Self {
            buffer: RwLock::new(Vec::new()),
        }
    }

    /// Drain all buffered Cypher deltas.
    pub fn drain(&self) -> Vec<CypherDelta> {
        let mut buf = self.buffer.write().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buf)
    }

    /// Project a Reflow Network topology change into Cypher.
    pub fn project_actor_added(&self, name: &str, template: &str, config: &Value) {
        self.push(CypherDelta {
            statement: "MERGE (a:Actor {name: $name}) SET a.template = $template, a.config = $config".into(),
            params: json!({"name": name, "template": template, "config": config}),
        });
    }

    pub fn project_actor_removed(&self, name: &str) {
        self.push(CypherDelta {
            statement: "MATCH (a:Actor {name: $name}) DETACH DELETE a".into(),
            params: json!({"name": name}),
        });
    }

    pub fn project_connection_added(
        &self,
        from_actor: &str,
        from_port: &str,
        to_actor: &str,
        to_port: &str,
    ) {
        self.push(CypherDelta {
            statement: concat!(
                "MATCH (a:Actor {name: $from}), (b:Actor {name: $to}) ",
                "MERGE (a)-[:CONNECTS_TO {from_port: $fp, to_port: $tp}]->(b)"
            ).into(),
            params: json!({
                "from": from_actor, "fp": from_port,
                "to": to_actor, "tp": to_port,
            }),
        });
    }

    pub fn project_connection_removed(
        &self,
        from_actor: &str,
        from_port: &str,
        to_actor: &str,
        to_port: &str,
    ) {
        self.push(CypherDelta {
            statement: concat!(
                "MATCH (a:Actor {name: $from})-[r:CONNECTS_TO {from_port: $fp, to_port: $tp}]->(b:Actor {name: $to}) ",
                "DELETE r"
            ).into(),
            params: json!({
                "from": from_actor, "fp": from_port,
                "to": to_actor, "tp": to_port,
            }),
        });
    }

    fn push(&self, delta: CypherDelta) {
        if let Ok(mut buf) = self.buffer.write() {
            buf.push(delta);
        }
    }
}

impl DeltaListener for GraphProjection {
    fn on_delta(&self, delta: &Delta) {
        match delta.op {
            DeltaOp::Put | DeltaOp::Merge => {
                // MERGE entity node
                self.push(CypherDelta {
                    statement: "MERGE (e:Entity {name: $entity})".into(),
                    params: json!({"entity": &delta.entity}),
                });

                // MERGE component node + relationship
                if !delta.component.is_empty() {
                    self.push(CypherDelta {
                        statement: concat!(
                            "MERGE (c:Component {id: $id}) ",
                            "SET c.type = $type, c.entity = $entity, c.data = $data, c.updated = $ts ",
                            "WITH c ",
                            "MATCH (e:Entity {name: $entity}) ",
                            "MERGE (e)-[:HAS_COMPONENT]->(c)"
                        ).into(),
                        params: json!({
                            "id": &delta.id,
                            "type": &delta.component,
                            "entity": &delta.entity,
                            "data": delta.data.as_ref().unwrap_or(&Value::Null),
                            "ts": &delta.timestamp,
                        }),
                    });

                    // Extract references (e.g. camera.target = "player" → REFERENCES edge)
                    if let Some(ref data) = delta.data {
                        extract_references(&delta.entity, &delta.component, data, self);
                    }
                }
            }

            DeltaOp::Delete => {
                if !delta.component.is_empty() {
                    // Delete component + relationship
                    self.push(CypherDelta {
                        statement: "MATCH (c:Component {id: $id}) DETACH DELETE c".into(),
                        params: json!({"id": &delta.id}),
                    });
                } else {
                    // Delete entire entity
                    self.push(CypherDelta {
                        statement: concat!(
                            "MATCH (e:Entity {name: $name}) ",
                            "OPTIONAL MATCH (e)-[:HAS_COMPONENT]->(c:Component) ",
                            "DETACH DELETE e, c"
                        ).into(),
                        params: json!({"name": &delta.entity}),
                    });
                }
            }

            DeltaOp::Tag => {
                if let Some(ref tags) = delta.data {
                    self.push(CypherDelta {
                        statement: concat!(
                            "MATCH (n {id: $id}) ",
                            "SET n.tags = $tags"
                        ).into(),
                        params: json!({"id": &delta.id, "tags": tags}),
                    });
                }
            }

            DeltaOp::Spawn => {
                if let Some(ref data) = delta.data {
                    let template = data.get("template").and_then(|v| v.as_str()).unwrap_or("");
                    self.push(CypherDelta {
                        statement: concat!(
                            "MATCH (tpl:Entity {name: $template}) ",
                            "MERGE (e:Entity {name: $entity}) ",
                            "MERGE (e)-[:SPAWNED_FROM]->(tpl)"
                        ).into(),
                        params: json!({"template": template, "entity": &delta.entity}),
                    });
                }
            }

            DeltaOp::Destroy => {
                self.push(CypherDelta {
                    statement: concat!(
                        "MATCH (e:Entity {name: $name}) ",
                        "OPTIONAL MATCH (e)-[:HAS_COMPONENT]->(c:Component) ",
                        "DETACH DELETE e, c"
                    ).into(),
                    params: json!({"name": &delta.entity}),
                });
            }
        }
    }
}

/// Extract cross-entity references from component data.
/// e.g. camera.target = "player" → (camera_entity)-[:REFERENCES]->(player)
fn extract_references(
    entity: &str,
    component: &str,
    data: &Value,
    projection: &GraphProjection,
) {
    // Known reference fields by component type
    let ref_fields: &[&str] = match component {
        "camera" => &["target"],
        "billboard" => &["target"],
        "behavior" => return, // behaviors have complex var refs, skip for now
        "state_machine" => return,
        _ => return,
    };

    if let Value::Object(map) = data {
        for field in ref_fields {
            if let Some(Value::String(ref target)) = map.get(*field) {
                projection.push(CypherDelta {
                    statement: concat!(
                        "MATCH (src:Entity {name: $src}), (dst:Entity {name: $dst}) ",
                        "MERGE (src)-[:REFERENCES {component: $comp, field: $field}]->(dst)"
                    ).into(),
                    params: json!({
                        "src": entity,
                        "dst": target,
                        "comp": component,
                        "field": field,
                    }),
                });
            }
        }
    }
}
