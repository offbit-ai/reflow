//! Integration tests for reflow_assets: AssetDB, ECS components,
//! easing (via behavior), layout backend, queries.

use reflow_assets::layout::LayoutBackend;
use reflow_assets::*;
use serde_json::json;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// AssetDB: put / get / has / delete
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn put_and_get_binary() {
    let db = AssetDB::in_memory().unwrap();
    let data = vec![1u8, 2, 3, 4, 5];
    db.put("test:mesh", &data, json!({"stride": 24})).unwrap();

    assert!(db.has("test:mesh"));
    let asset = db.get("test:mesh").unwrap();
    assert_eq!(asset.data, data);
    assert_eq!(asset.entry.metadata["stride"], 24);
}

#[test]
fn put_and_get_json() {
    let db = AssetDB::in_memory().unwrap();
    db.put_json(
        "gold:material",
        json!({"albedo": [1.0, 0.84, 0.0], "metallic": 1.0, "roughness": 0.3}),
        json!({}),
    )
    .unwrap();

    let asset = db.get("gold:material").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&asset.data).unwrap();
    assert_eq!(v["metallic"], 1.0);
    assert_eq!(v["albedo"][0], 1.0);
}

#[test]
fn upsert_replaces_data() {
    let db = AssetDB::in_memory().unwrap();
    db.put("item:mesh", &[1, 2, 3], json!({})).unwrap();
    db.put("item:mesh", &[4, 5, 6, 7], json!({"version": 2}))
        .unwrap();

    let asset = db.get("item:mesh").unwrap();
    assert_eq!(asset.data, vec![4, 5, 6, 7]);
    assert_eq!(asset.entry.metadata["version"], 2);
}

#[test]
fn has_returns_false_for_missing() {
    let db = AssetDB::in_memory().unwrap();
    assert!(!db.has("nonexistent:mesh"));
}

#[test]
fn delete_removes_entity() {
    let db = AssetDB::in_memory().unwrap();
    db.put("temp:mesh", &[1, 2], json!({})).unwrap();
    assert!(db.has("temp:mesh"));

    db.delete("temp:mesh").unwrap();
    assert!(!db.has("temp:mesh"));
    assert!(db.get("temp:mesh").is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// AssetDB: tags
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn tag_and_query_by_tag() {
    let db = AssetDB::in_memory().unwrap();
    db.put("sword:mesh", &[1], json!({})).unwrap();
    db.tag("sword:mesh", &["weapon", "melee"]).unwrap();

    db.put("bow:mesh", &[2], json!({})).unwrap();
    db.tag("bow:mesh", &["weapon", "ranged"]).unwrap();

    db.put("potion:mesh", &[3], json!({})).unwrap();
    db.tag("potion:mesh", &["consumable"]).unwrap();

    // Query all weapons
    let results = db.query_dsl(&json!({"tags": ["weapon"]})).unwrap();
    assert_eq!(results.len(), 2);

    // Query melee weapons
    let results = db
        .query_dsl(&json!({"tags": {"$all": ["weapon", "melee"]}}))
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "sword");
}

// ═══════════════════════════════════════════════════════════════════════════
// AssetDB: document-style query DSL
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn query_by_type() {
    let db = AssetDB::in_memory().unwrap();
    db.put("a:mesh", &[1], json!({})).unwrap();
    db.put("b:mesh", &[2], json!({})).unwrap();
    db.put("c:texture", &[3], json!({})).unwrap();

    let meshes = db.query_dsl(&json!({"type": "mesh"})).unwrap();
    assert_eq!(meshes.len(), 2);
}

#[test]
fn query_by_name_contains() {
    let db = AssetDB::in_memory().unwrap();
    db.put("snake_body:mesh", &[1], json!({})).unwrap();
    db.put("snake_head:mesh", &[2], json!({})).unwrap();
    db.put("tree_trunk:mesh", &[3], json!({})).unwrap();

    let results = db
        .query_dsl(&json!({"name": {"$contains": "snake"}}))
        .unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn query_by_size() {
    let db = AssetDB::in_memory().unwrap();
    db.put("small:generic", &[1, 2], json!({})).unwrap();
    // Use random-ish data so LZ4 can't compress it much
    let big_data: Vec<u8> = (0..1000).map(|i| (i * 37 % 256) as u8).collect();
    db.put("big:generic", &big_data, json!({})).unwrap();

    // big blob_size should be > 100 even after compression
    let entry = db.get_entry("big:generic").unwrap();
    assert!(entry.blob_size > 100);

    let results = db.query_dsl(&json!({"size": {"$gt": 100}})).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].name.contains("big"));
}

#[test]
fn query_with_limit_and_sort() {
    let db = AssetDB::in_memory().unwrap();
    for i in 0..5 {
        db.put(
            &format!("item_{}:mesh", i),
            &vec![0u8; (i + 1) * 100],
            json!({}),
        )
        .unwrap();
    }

    let results = db
        .query_dsl(&json!({"type": "mesh", "$sort": "largest", "$limit": 2}))
        .unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].blob_size >= results[1].blob_size);
}

// ═══════════════════════════════════════════════════════════════════════════
// ECS: component access
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn set_and_get_component() {
    let db = AssetDB::in_memory().unwrap();

    db.set_component_json(
        "player",
        "transform",
        json!({"position": [0, 1, 0], "rotation": [0, 0, 0, 1]}),
        json!({}),
    )
    .unwrap();

    assert!(db.has_component("player", "transform"));
    assert!(!db.has_component("player", "rigidbody"));

    let asset = db.get_component("player", "transform").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&asset.data).unwrap();
    assert_eq!(v["position"][1], 1);
}

#[test]
fn components_of_entity() {
    let db = AssetDB::in_memory().unwrap();
    db.set_component_json("enemy", "transform", json!({}), json!({}))
        .unwrap();
    db.set_component_json("enemy", "rigidbody", json!({}), json!({}))
        .unwrap();
    db.set_component_json("enemy", "collider", json!({}), json!({}))
        .unwrap();

    let mut comps = db.components_of("enemy").unwrap();
    comps.sort();
    assert_eq!(comps, vec!["collider", "rigidbody", "transform"]);
}

#[test]
fn entities_with_all_components() {
    let db = AssetDB::in_memory().unwrap();

    // player has transform + rigidbody
    db.set_component_json("player", "transform", json!({}), json!({}))
        .unwrap();
    db.set_component_json("player", "rigidbody", json!({}), json!({}))
        .unwrap();

    // wall has transform only
    db.set_component_json("wall", "transform", json!({}), json!({}))
        .unwrap();

    // enemy has transform + rigidbody + ai
    db.set_component_json("enemy", "transform", json!({}), json!({}))
        .unwrap();
    db.set_component_json("enemy", "rigidbody", json!({}), json!({}))
        .unwrap();
    db.set_component_json("enemy", "ai", json!({}), json!({}))
        .unwrap();

    let mut dynamic = db.entities_with(&["transform", "rigidbody"]).unwrap();
    dynamic.sort();
    assert_eq!(dynamic, vec!["enemy", "player"]);
}

#[test]
fn entity_snapshot() {
    let db = AssetDB::in_memory().unwrap();
    db.set_component_json(
        "npc",
        "transform",
        json!({"position": [5, 0, 3]}),
        json!({}),
    )
    .unwrap();
    db.set_component_json("npc", "health", json!({"hp": 100, "max": 100}), json!({}))
        .unwrap();

    let snap = db.entity_snapshot("npc").unwrap();
    assert_eq!(snap["transform"]["position"][0], 5);
    assert_eq!(snap["health"]["hp"], 100);
}

#[test]
fn spawn_from_template() {
    let db = AssetDB::in_memory().unwrap();

    // Template
    db.set_component_json(
        "crate_tpl",
        "transform",
        json!({"position": [0, 0, 0]}),
        json!({}),
    )
    .unwrap();
    db.set_component_json("crate_tpl", "rigidbody", json!({"mass": 10}), json!({}))
        .unwrap();
    db.set_component_json(
        "crate_tpl",
        "material",
        json!({"albedo": [0.6, 0.4, 0.2]}),
        json!({}),
    )
    .unwrap();

    // Spawn
    db.spawn_from("crate_tpl", "crate_42").unwrap();

    // Verify all components copied
    assert!(db.has_component("crate_42", "transform"));
    assert!(db.has_component("crate_42", "rigidbody"));
    assert!(db.has_component("crate_42", "material"));

    let rb = db.get_component("crate_42", "rigidbody").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&rb.data).unwrap();
    assert_eq!(v["mass"], 10);

    // Modify clone without affecting template
    db.set_component_json(
        "crate_42",
        "transform",
        json!({"position": [5, 10, 0]}),
        json!({}),
    )
    .unwrap();
    let tpl_tf = db.get_component("crate_tpl", "transform").unwrap();
    let tv: serde_json::Value = serde_json::from_slice(&tpl_tf.data).unwrap();
    assert_eq!(tv["position"][0], 0); // template unchanged
}

#[test]
fn destroy_entity_removes_all_components() {
    let db = AssetDB::in_memory().unwrap();
    db.set_component_json("doomed", "transform", json!({}), json!({}))
        .unwrap();
    db.set_component_json("doomed", "mesh", json!({}), json!({}))
        .unwrap();
    db.set_component_json("doomed", "material", json!({}), json!({}))
        .unwrap();

    db.destroy_entity("doomed").unwrap();

    assert!(!db.has_component("doomed", "transform"));
    assert!(!db.has_component("doomed", "mesh"));
    assert!(db.components_of("doomed").unwrap().is_empty());
}

#[test]
fn remove_single_component() {
    let db = AssetDB::in_memory().unwrap();
    db.set_component_json("player", "transform", json!({}), json!({}))
        .unwrap();
    db.set_component_json("player", "flying", json!({}), json!({}))
        .unwrap();

    db.remove_component("player", "flying").unwrap();

    assert!(db.has_component("player", "transform"));
    assert!(!db.has_component("player", "flying"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Layout backend: headless
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn headless_layout_hydrate_and_query() {
    let db = AssetDB::in_memory().unwrap();

    // Create a DOM entity
    db.set_component_json(
        "header",
        "dom",
        json!({"tag": "div", "width": 800, "height": 60}),
        json!({}),
    )
    .unwrap();
    db.set_component_json(
        "header",
        "transform",
        json!({"position": [0, 0, 0]}),
        json!({}),
    )
    .unwrap();

    let backend = layout::HeadlessLayoutBackend::new();
    backend.hydrate(&db).unwrap();

    assert_eq!(backend.query("header", "width"), Some(800.0));
    assert_eq!(backend.query("header", "height"), Some(60.0));
    assert_eq!(
        backend.query_string("header", "tag"),
        Some("div".to_string())
    );
}

#[test]
fn headless_layout_sync_reads_transform_changes() {
    let db = AssetDB::in_memory().unwrap();

    db.set_component_json(
        "box",
        "dom",
        json!({"tag": "div", "width": 100, "height": 100}),
        json!({}),
    )
    .unwrap();
    db.set_component_json(
        "box",
        "transform",
        json!({"position": [0, 0, 0]}),
        json!({}),
    )
    .unwrap();

    let backend = layout::HeadlessLayoutBackend::new();
    backend.hydrate(&db).unwrap();

    assert_eq!(backend.query("box", "x"), Some(0.0));

    // Systems update position in AssetDB
    db.set_component_json(
        "box",
        "transform",
        json!({"position": [50, 100, 0]}),
        json!({}),
    )
    .unwrap();

    // Sync reads it back
    backend.sync(&db).unwrap();

    assert_eq!(backend.query("box", "x"), Some(50.0));
    assert_eq!(backend.query("box", "y"), Some(100.0));
}

#[test]
fn headless_layout_hit_test() {
    let backend = layout::HeadlessLayoutBackend::new();
    backend.set_node(
        "btn",
        layout::LayoutNode {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 40.0,
            ..Default::default()
        },
    );

    assert_eq!(backend.hit_test(50.0, 25.0), Some("btn".to_string()));
    assert_eq!(backend.hit_test(200.0, 200.0), None);
}

#[test]
fn headless_scroll_progress() {
    let backend = layout::HeadlessLayoutBackend::new();
    backend.set_node(
        "page",
        layout::LayoutNode {
            height: 600.0,
            scroll_y: 300.0,
            scroll_height: 2400.0,
            ..Default::default()
        },
    );

    let progress = backend.query("page", "scrollProgress").unwrap();
    // 300 / (2400 - 600) = 300/1800 ≈ 0.1667
    assert!((progress - 0.1667).abs() < 0.001);
}

// ═══════════════════════════════════════════════════════════════════════════
// Layout backend: @layout() resolution
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn layout_query_resolution() {
    let backend = Arc::new(layout::HeadlessLayoutBackend::new());
    backend.set_node(
        "container",
        layout::LayoutNode {
            width: 1920.0,
            height: 1080.0,
            scroll_y: 540.0,
            scroll_height: 5400.0,
            ..Default::default()
        },
    );

    layout::set_layout_backend("test_db", backend);

    let scroll = layout::resolve_layout_query("test_db", "container:scrollProgress");
    assert!(scroll.is_some());

    let width = layout::resolve_layout_query("test_db", "container:width");
    assert_eq!(width, Some(1920.0));

    // Missing entity
    let missing = layout::resolve_layout_query("test_db", "nonexistent:width");
    assert!(missing.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Content-addressed deduplication
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn identical_data_deduplicates() {
    let db = AssetDB::in_memory().unwrap();
    let data = vec![42u8; 1024];

    db.put("mesh_a:mesh", &data, json!({})).unwrap();
    db.put("mesh_b:mesh", &data, json!({})).unwrap();

    let a = db.get_entry("mesh_a:mesh").unwrap();
    let b = db.get_entry("mesh_b:mesh").unwrap();

    // Same content → same hash → same blob
    assert_eq!(a.blob_hash, b.blob_hash);
}

// ═══════════════════════════════════════════════════════════════════════════
// Stats
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stats_reports_counts() {
    let db = AssetDB::in_memory().unwrap();
    db.put("a:mesh", &[1], json!({})).unwrap();
    db.put("b:mesh", &[2], json!({})).unwrap();
    db.put("c:texture", &[3], json!({})).unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(stats["assetCount"], 3);
    assert_eq!(stats["types"]["mesh"], 2);
    assert_eq!(stats["types"]["texture"], 1);
}
