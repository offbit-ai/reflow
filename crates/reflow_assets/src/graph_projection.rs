//! Knowledge graph projection — projects AssetDB + Reflow Network
//! into KyuGraph, a property graph database queryable via Cypher.
//!
//! Designed around KyuGraph's delta upsert protocol: every AssetDB
//! mutation emits a delta that maps to a Cypher MERGE/DELETE statement.
//! KyuGraph applies these incrementally — no full graph rebuilds.
//!
//! KyuGraph supports in-memory (development) and S3-backed (production)
//! storage, so the knowledge graph persists across sessions when needed.
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
//! ## Setup
//!
//! ```ignore
//! // 1. Initialize KyuGraph schema (once)
//! for ddl in GraphProjection::schema_ddl() {
//!     kyu.execute(ddl)?;
//! }
//!
//! // 2. Register projection as AssetDB delta listener
//! let projector = Arc::new(GraphProjection::new());
//! db.add_listener(projector.clone());
//!
//! // 3. Drain deltas and apply to KyuGraph each tick (or on demand)
//! for delta in projector.drain() {
//!     kyu.execute_with_params(&delta.statement, &delta.params)?;
//! }
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

    /// Schema DDL — run these once on KyuGraph initialization to create
    /// the node and relationship tables. KyuGraph requires typed schema.
    pub fn schema_ddl() -> Vec<&'static str> {
        vec![
            // Node tables
            "CREATE NODE TABLE IF NOT EXISTS Entity (name STRING, created_at STRING, PRIMARY KEY (name))",
            "CREATE NODE TABLE IF NOT EXISTS Component (id STRING, type STRING, entity STRING, data STRING, updated STRING, PRIMARY KEY (id))",
            "CREATE NODE TABLE IF NOT EXISTS Actor (name STRING, template STRING, config STRING, PRIMARY KEY (name))",

            // Relationship tables
            "CREATE REL TABLE IF NOT EXISTS HAS_COMPONENT (FROM Entity TO Component)",
            "CREATE REL TABLE IF NOT EXISTS CONNECTS_TO (FROM Actor TO Actor, from_port STRING, to_port STRING)",
            "CREATE REL TABLE IF NOT EXISTS TARGETS (FROM Actor TO Entity)",
            "CREATE REL TABLE IF NOT EXISTS REFERENCES (FROM Entity TO Entity, component STRING, field STRING)",
            "CREATE REL TABLE IF NOT EXISTS SPAWNED_FROM (FROM Entity TO Entity)",
            "CREATE REL TABLE IF NOT EXISTS TAGGED (FROM Entity TO Entity)",
        ]
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
///
/// Scans all string values in the component JSON. Any string that looks
/// like an entity reference (contains no spaces, no special chars besides
/// `:`, `_`, `-`, `/`) is treated as a potential entity reference.
///
/// This is generic — works for any domain, not just game components.
/// camera.target = "player", behavior.vars.source = "sensor_1:data",
/// http.upstream = "api_gateway", etc.
fn extract_references(
    entity: &str,
    component: &str,
    data: &Value,
    projection: &GraphProjection,
) {
    if let Value::Object(map) = data {
        for (field, val) in map {
            extract_refs_recursive(entity, component, field, val, projection);
        }
    }
}

fn extract_refs_recursive(
    entity: &str,
    component: &str,
    field: &str,
    value: &Value,
    projection: &GraphProjection,
) {
    match value {
        Value::String(s) => {
            // Heuristic: looks like an entity reference if it's a simple
            // identifier (alphanumeric + _ - / :) and not a URL, path, or expression
            let s = s.trim();
            if !s.is_empty()
                && !s.contains(' ')
                && !s.starts_with('/')
                && !s.starts_with("http")
                && !s.starts_with('@')
                && !s.starts_with('$')
                && !s.contains('(')
                && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '/')
                && s != entity // don't self-reference
            {
                // Extract the entity name (before ':' if present)
                let target_entity = if let Some(colon) = s.rfind(':') {
                    &s[..colon]
                } else {
                    s
                };

                projection.push(CypherDelta {
                    statement: concat!(
                        "MATCH (src:Entity {name: $src}), (dst:Entity {name: $dst}) ",
                        "MERGE (src)-[:REFERENCES {component: $comp, field: $field}]->(dst)"
                    ).into(),
                    params: json!({
                        "src": entity,
                        "dst": target_entity,
                        "comp": component,
                        "field": field,
                    }),
                });
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                extract_refs_recursive(entity, component, &format!("{}.{}", field, k), v, projection);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                extract_refs_recursive(entity, component, &format!("{}[{}]", field, i), v, projection);
            }
        }
        _ => {}
    }
}
