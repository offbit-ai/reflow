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
use std::sync::RwLock;

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

impl Default for GraphProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphProjection {
    pub fn new() -> Self {
        Self {
            buffer: RwLock::new(Vec::new()),
        }
    }

    /// Schema DDL — run these once on KyuGraph initialization to create
    /// the node and relationship tables. KyuGraph requires typed schema.
    ///
    /// Organized by knowledge boundary:
    /// - **World**: Entity, Component (what exists)
    /// - **Skills**: ActorTemplate, Actor (how to do things)
    /// - **Results**: RenderResult, QualityScore (what happened)
    /// - **Diagnostics**: Trace, Warning (what went wrong)
    pub fn schema_ddl() -> Vec<&'static str> {
        vec![
            // ─── World knowledge ───
            "CREATE NODE TABLE IF NOT EXISTS Entity (name STRING, created_at STRING, PRIMARY KEY (name))",
            "CREATE NODE TABLE IF NOT EXISTS Component (id STRING, type STRING, entity STRING, data STRING, updated STRING, PRIMARY KEY (id))",
            "CREATE REL TABLE IF NOT EXISTS HAS_COMPONENT (FROM Entity TO Component)",
            "CREATE REL TABLE IF NOT EXISTS REFERENCES (FROM Entity TO Entity, component STRING, field STRING)",
            "CREATE REL TABLE IF NOT EXISTS SPAWNED_FROM (FROM Entity TO Entity)",

            // ─── Skills knowledge ───
            "CREATE NODE TABLE IF NOT EXISTS Actor (name STRING, template STRING, config STRING, PRIMARY KEY (name))",
            "CREATE NODE TABLE IF NOT EXISTS ActorTemplate (id STRING, name STRING, inports STRING, outports STRING, description STRING, category STRING, PRIMARY KEY (id))",
            "CREATE REL TABLE IF NOT EXISTS CONNECTS_TO (FROM Actor TO Actor, from_port STRING, to_port STRING)",
            "CREATE REL TABLE IF NOT EXISTS TARGETS (FROM Actor TO Entity)",
            "CREATE REL TABLE IF NOT EXISTS INSTANCE_OF (FROM Actor TO ActorTemplate)",
            // Wiring patterns: known good DAG fragments
            "CREATE NODE TABLE IF NOT EXISTS WiringPattern (id STRING, name STRING, description STRING, actors STRING, connections STRING, PRIMARY KEY (id))",
            "CREATE REL TABLE IF NOT EXISTS USES_TEMPLATE (FROM WiringPattern TO ActorTemplate)",

            // ─── Results knowledge ───
            "CREATE NODE TABLE IF NOT EXISTS RenderResult (id STRING, entity STRING, timestamp STRING, image_path STRING, width INT64, height INT64, PRIMARY KEY (id))",
            "CREATE NODE TABLE IF NOT EXISTS QualityScore (id STRING, render_id STRING, score DOUBLE, analysis STRING, timestamp STRING, PRIMARY KEY (id))",
            "CREATE REL TABLE IF NOT EXISTS RENDERED_BY (FROM RenderResult TO Entity)",
            "CREATE REL TABLE IF NOT EXISTS SCORED (FROM QualityScore TO RenderResult)",

            // ─── Diagnostics knowledge ───
            "CREATE NODE TABLE IF NOT EXISTS Trace (id STRING, actor STRING, timestamp STRING, duration_ms DOUBLE, input_keys STRING, output_keys STRING, PRIMARY KEY (id))",
            "CREATE NODE TABLE IF NOT EXISTS Warning (id STRING, entity STRING, type STRING, message STRING, timestamp STRING, PRIMARY KEY (id))",
            "CREATE REL TABLE IF NOT EXISTS TRACED_BY (FROM Trace TO Actor)",
            "CREATE REL TABLE IF NOT EXISTS WARNS_ABOUT (FROM Warning TO Entity)",
        ]
    }

    /// Project the actor template catalog into the graph.
    /// Call once at startup with all registered template IDs.
    /// AI agents use this to discover what actors are available and how to wire them.
    pub fn project_catalog(
        &self,
        templates: &[(String, String, Vec<String>, Vec<String>)], // (template_id, actor_name, inports, outports)
    ) {
        // Schema: ActorTemplate node
        self.push(CypherDelta {
            statement: "CREATE NODE TABLE IF NOT EXISTS ActorTemplate (id STRING, name STRING, inports STRING, outports STRING, PRIMARY KEY (id))".into(),
            params: json!({}),
        });
        self.push(CypherDelta {
            statement: "CREATE REL TABLE IF NOT EXISTS INSTANCE_OF (FROM Actor TO ActorTemplate)"
                .into(),
            params: json!({}),
        });

        for (template_id, actor_name, inports, outports) in templates {
            self.push(CypherDelta {
                statement: concat!(
                    "MERGE (t:ActorTemplate {id: $id}) ",
                    "SET t.name = $name, t.inports = $inports, t.outports = $outports"
                )
                .into(),
                params: json!({
                    "id": template_id,
                    "name": actor_name,
                    "inports": serde_json::to_string(inports).unwrap_or_default(),
                    "outports": serde_json::to_string(outports).unwrap_or_default(),
                }),
            });
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
            statement:
                "MERGE (a:Actor {name: $name}) SET a.template = $template, a.config = $config"
                    .into(),
            params: json!({"name": name, "template": template, "config": config}),
        });
        // Link to template catalog
        self.push(CypherDelta {
            statement: concat!(
                "MATCH (a:Actor {name: $name}), (t:ActorTemplate {id: $template}) ",
                "MERGE (a)-[:INSTANCE_OF]->(t)"
            )
            .into(),
            params: json!({"name": name, "template": template}),
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
            )
            .into(),
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

    // ─── Results knowledge ───

    /// Record a render result — the visual feedback loop.
    /// VLM agents read these to evaluate quality.
    pub fn project_render_result(&self, entity: &str, image_path: &str, width: u32, height: u32) {
        let id = format!("render_{}_{}", entity, now_iso_compact());
        self.push(CypherDelta {
            statement: concat!(
                "CREATE (r:RenderResult {id: $id, entity: $entity, timestamp: $ts, ",
                "image_path: $path, width: $w, height: $h}) ",
                "WITH r MATCH (e:Entity {name: $entity}) ",
                "CREATE (r)-[:RENDERED_BY]->(e)"
            )
            .into(),
            params: json!({
                "id": id,
                "entity": entity,
                "ts": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                "path": image_path,
                "w": width,
                "h": height,
            }),
        });
    }

    /// Record a VLM quality assessment of a render.
    pub fn project_quality_score(&self, render_id: &str, score: f64, analysis: &str) {
        let id = format!("score_{}", now_iso_compact());
        self.push(CypherDelta {
            statement: concat!(
                "CREATE (q:QualityScore {id: $id, render_id: $rid, score: $score, ",
                "analysis: $analysis, timestamp: $ts}) ",
                "WITH q MATCH (r:RenderResult {id: $rid}) ",
                "CREATE (q)-[:SCORED]->(r)"
            )
            .into(),
            params: json!({
                "id": id,
                "rid": render_id,
                "score": score,
                "analysis": analysis,
                "ts": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            }),
        });
    }

    // ─── Diagnostics knowledge ───

    /// Record an actor execution trace.
    pub fn project_trace(
        &self,
        actor: &str,
        duration_ms: f64,
        input_keys: &[String],
        output_keys: &[String],
    ) {
        let id = format!("trace_{}_{}", actor, now_iso_compact());
        self.push(CypherDelta {
            statement: concat!(
                "CREATE (t:Trace {id: $id, actor: $actor, timestamp: $ts, ",
                "duration_ms: $dur, input_keys: $ik, output_keys: $ok}) ",
                "WITH t MATCH (a:Actor {name: $actor}) ",
                "CREATE (t)-[:TRACED_BY]->(a)"
            )
            .into(),
            params: json!({
                "id": id,
                "actor": actor,
                "ts": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                "dur": duration_ms,
                "ik": serde_json::to_string(input_keys).unwrap_or_default(),
                "ok": serde_json::to_string(output_keys).unwrap_or_default(),
            }),
        });
    }

    /// Record a diagnostic warning.
    pub fn project_warning(&self, entity: &str, warning_type: &str, message: &str) {
        let id = format!("warn_{}_{}", entity, now_iso_compact());
        self.push(CypherDelta {
            statement: concat!(
                "CREATE (w:Warning {id: $id, entity: $entity, type: $wtype, ",
                "message: $msg, timestamp: $ts}) ",
                "WITH w MATCH (e:Entity {name: $entity}) ",
                "CREATE (w)-[:WARNS_ABOUT]->(e)"
            )
            .into(),
            params: json!({
                "id": id,
                "entity": entity,
                "wtype": warning_type,
                "msg": message,
                "ts": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            }),
        });
    }

    // ─── Skills knowledge: wiring patterns ───

    /// Register a known-good wiring pattern (DAG fragment).
    /// AI agents reference these when building DAGs for common tasks.
    pub fn project_wiring_pattern(
        &self,
        id: &str,
        name: &str,
        description: &str,
        actors: &[(&str, &str)],                  // (name, template)
        connections: &[(&str, &str, &str, &str)], // (from_actor, from_port, to_actor, to_port)
    ) {
        self.push(CypherDelta {
            statement: concat!(
                "MERGE (p:WiringPattern {id: $id}) ",
                "SET p.name = $name, p.description = $desc, ",
                "p.actors = $actors, p.connections = $conns"
            )
            .into(),
            params: json!({
                "id": id,
                "name": name,
                "desc": description,
                "actors": serde_json::to_string(actors).unwrap_or_default(),
                "conns": serde_json::to_string(connections).unwrap_or_default(),
            }),
        });

        // Link to templates used
        for (_, template) in actors {
            self.push(CypherDelta {
                statement: concat!(
                    "MATCH (p:WiringPattern {id: $pid}), (t:ActorTemplate {id: $tid}) ",
                    "MERGE (p)-[:USES_TEMPLATE]->(t)"
                )
                .into(),
                params: json!({"pid": id, "tid": template}),
            });
        }
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
                        )
                        .into(),
                        params: json!({"name": &delta.entity}),
                    });
                }
            }

            DeltaOp::Tag => {
                if let Some(ref tags) = delta.data {
                    self.push(CypherDelta {
                        statement: concat!("MATCH (n {id: $id}) ", "SET n.tags = $tags").into(),
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
                        )
                        .into(),
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
                    )
                    .into(),
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
fn now_iso_compact() -> String {
    chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string()
}

fn extract_references(entity: &str, component: &str, data: &Value, projection: &GraphProjection) {
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
                && s.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '/')
                && s != entity
            // don't self-reference
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
                    )
                    .into(),
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
                extract_refs_recursive(
                    entity,
                    component,
                    &format!("{}.{}", field, k),
                    v,
                    projection,
                );
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                extract_refs_recursive(
                    entity,
                    component,
                    &format!("{}[{}]", field, i),
                    v,
                    projection,
                );
            }
        }
        _ => {}
    }
}
