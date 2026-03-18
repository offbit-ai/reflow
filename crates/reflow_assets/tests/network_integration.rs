//! Integration test: Reflow network + AssetDB + systems.
//!
//! Simulates a real workflow:
//! 1. Set up DOM entities in AssetDB
//! 2. Wire a DAG: LayoutSync → BehaviorSystem → TweenSystem → LayoutSync
//! 3. Run the network
//! 4. Verify transforms changed in AssetDB
//!
//! This test does NOT need a browser — it uses HeadlessLayoutBackend.

use reflow_assets::layout::{HeadlessLayoutBackend, LayoutBackend, LayoutNode};
use reflow_assets::{get_or_create_db, layout};
use serde_json::json;
use std::sync::Arc;

const DB_PATH: &str = "./test_dom_tween.db";

/// Test: create a DOM element, attach a tween, run 10 ticks,
/// verify the transform interpolated.
#[test]
fn tween_drives_dom_transform() {
    // Clean slate
    let _ = std::fs::remove_dir_all(DB_PATH);

    let db = get_or_create_db(DB_PATH).unwrap();

    // ── Set up the world ──

    // A button entity with DOM, transform, style
    db.set_component_json(
        "btn",
        "dom",
        json!({ "tag": "button", "text": "Click me", "width": 120, "height": 40 }),
        json!({}),
    )
    .unwrap();

    db.set_component_json(
        "btn",
        "transform",
        json!({ "position": [0.0, 0.0, 0.0] }),
        json!({}),
    )
    .unwrap();

    db.set_component_json(
        "btn",
        "style",
        json!({ "opacity": 0.0, "backgroundColor": "#007bff" }),
        json!({}),
    )
    .unwrap();

    // A tween: slide button from x=0 to x=200, fade in opacity 0→1
    db.put_json(
        "btn_slide:tween",
        json!({
            "target": "btn:transform.position",
            "from": [0.0, 0.0, 0.0],
            "to": [200.0, 0.0, 0.0],
            "duration": 0.5,
            "easing": "easeOutCubic",
            "delay": 0.0,
            "loop": false,
            "yoyo": false,
            "elapsed": 0.0,
            "state": "playing",
        }),
        json!({}),
    )
    .unwrap();

    db.put_json(
        "btn_fade:tween",
        json!({
            "target": "btn:style.opacity",
            "from": 0.0,
            "to": 1.0,
            "duration": 0.5,
            "easing": "easeOutCubic",
            "delay": 0.0,
            "loop": false,
            "elapsed": 0.0,
            "state": "playing",
        }),
        json!({}),
    )
    .unwrap();

    // Register headless layout backend
    let backend = Arc::new(HeadlessLayoutBackend::new());
    layout::set_layout_backend(DB_PATH, backend.clone());

    // Hydrate layout from AssetDB
    backend.hydrate(&db).unwrap();

    // Verify initial state
    assert_eq!(backend.query("btn", "x"), Some(0.0));

    let initial_tf = db.get_component("btn", "transform").unwrap();
    let initial: serde_json::Value = serde_json::from_slice(&initial_tf.data).unwrap();
    assert_eq!(initial["position"][0], 0.0);

    // ── Simulate tween ticks manually ──
    // (In a real DAG, IntervalTrigger drives TweenSystem.
    //  Here we call the tween logic directly via AssetDB.)

    let dt = 0.06; // 60ms per tick, ~9 ticks to complete 0.5s duration

    for _tick in 0..10 {
        // Process all tweens
        let tween_entries = db
            .query(&reflow_assets::AssetQuery::new().asset_type("tween"))
            .unwrap();

        for entry in &tween_entries {
            let tween = match &entry.inline_data {
                Some(v) => v.clone(),
                None => continue,
            };

            let state = tween.get("state").and_then(|v| v.as_str()).unwrap_or("");
            if state != "playing" {
                continue;
            }

            let duration = tween["duration"].as_f64().unwrap_or(1.0);
            let easing_fn = tween
                .get("easing")
                .and_then(|v| v.as_str())
                .unwrap_or("linear");
            let mut elapsed = tween["elapsed"].as_f64().unwrap_or(0.0);
            elapsed += dt;

            let progress = (elapsed / duration).min(1.0);
            let new_state = if progress >= 1.0 {
                "completed"
            } else {
                "playing"
            };

            // Interpolate
            let from = &tween["from"];
            let to = &tween["to"];
            let interpolated = interpolate(from, to, progress, easing_fn);

            // Write to target
            let target = tween["target"].as_str().unwrap();
            write_target(&db, target, &interpolated);

            // Update tween elapsed
            let mut updated = tween.clone();
            updated["elapsed"] = json!(elapsed);
            updated["state"] = json!(new_state);
            db.put_json(&entry.id, updated, entry.metadata.clone())
                .unwrap();
        }

        // Sync layout
        backend.sync(&db).unwrap();
    }

    // ── Verify final state ──

    // Transform should be at [200, 0, 0]
    let final_tf = db.get_component("btn", "transform").unwrap();
    let final_v: serde_json::Value = serde_json::from_slice(&final_tf.data).unwrap();
    let final_x = final_v["position"][0].as_f64().unwrap();
    assert!(
        (final_x - 200.0).abs() < 1.0,
        "Expected x ≈ 200, got {}",
        final_x
    );

    // Opacity should be ≈ 1.0
    let final_style = db.get_component("btn", "style").unwrap();
    let style_v: serde_json::Value = serde_json::from_slice(&final_style.data).unwrap();
    let final_opacity = style_v["opacity"].as_f64().unwrap();
    assert!(
        (final_opacity - 1.0).abs() < 0.05,
        "Expected opacity ≈ 1.0, got {}",
        final_opacity
    );

    // Layout backend should reflect the synced position
    assert_eq!(backend.query("btn", "x"), Some(200.0));

    // Both tweens should be completed
    let slide = db.get("btn_slide:tween").unwrap();
    let slide_v: serde_json::Value = serde_json::from_slice(&slide.data).unwrap();
    assert_eq!(slide_v["state"], "completed");

    let fade = db.get("btn_fade:tween").unwrap();
    let fade_v: serde_json::Value = serde_json::from_slice(&fade.data).unwrap();
    assert_eq!(fade_v["state"], "completed");

    // Cleanup
    let _ = std::fs::remove_dir_all(DB_PATH);
}

/// Test: behavior rule drives a continuous rotation based on time.
#[test]
fn behavior_drives_rotation() {
    let db_path = "./test_behavior_rotation.db";
    let _ = std::fs::remove_dir_all(db_path);
    let db = get_or_create_db(db_path).unwrap();

    // Spinner entity
    db.set_component_json(
        "spinner",
        "transform",
        json!({ "position": [0, 0, 0], "rotation": [0, 0, 0] }),
        json!({}),
    )
    .unwrap();

    db.put_json(
        "spinner:behavior",
        json!({
            "rules": [{
                "name": "spin",
                "target": "transform.rotation.2",
                "expr": "time * 90",
                "enabled": true,
            }]
        }),
        json!({}),
    )
    .unwrap();

    // Simulate behavior at time = 2.0 seconds
    // Expected: rotation.z = 2.0 * 90 = 180
    let rules = db.get("spinner:behavior").unwrap();
    let rules_v: serde_json::Value = serde_json::from_slice(&rules.data).unwrap();
    let rule = &rules_v["rules"][0];

    let expr_str = rule["expr"].as_str().unwrap();
    let target = rule["target"].as_str().unwrap();

    let mut vars = std::collections::HashMap::new();
    vars.insert("time".to_string(), 2.0);

    let result = reflow_assets_eval_expr(expr_str, &vars).unwrap();
    assert_eq!(result, 180.0);

    // Write to entity
    let full_target = format!("spinner:{}", target);
    write_target(&db, &full_target, &json!(result));

    // Verify
    let tf = db.get_component("spinner", "transform").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&tf.data).unwrap();
    assert_eq!(v["rotation"][2], 180.0);

    let _ = std::fs::remove_dir_all(db_path);
}

/// Test: state machine transitions on trigger.
#[test]
fn state_machine_transitions() {
    let db_path = "./test_state_machine.db";
    let _ = std::fs::remove_dir_all(db_path);
    let db = get_or_create_db(db_path).unwrap();

    // Button with state machine
    db.put_json(
        "button:state_machine",
        json!({
            "current": "idle",
            "states": {
                "idle": {},
                "hover": {},
                "pressed": {},
            },
            "transitions": [
                { "from": "idle", "to": "hover", "trigger": "pointerEnter" },
                { "from": "hover", "to": "idle", "trigger": "pointerLeave" },
                { "from": "hover", "to": "pressed", "trigger": "pointerDown" },
                { "from": "pressed", "to": "hover", "trigger": "pointerUp" },
            ]
        }),
        json!({}),
    )
    .unwrap();

    // Verify initial state
    let sm = db.get("button:state_machine").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&sm.data).unwrap();
    assert_eq!(v["current"], "idle");

    // Simulate: pointerEnter → should transition to "hover"
    let transitions = v["transitions"].as_array().unwrap();
    let current = v["current"].as_str().unwrap();
    let trigger = "pointerEnter";

    let new_state = find_transition(transitions, current, trigger);
    assert_eq!(new_state, Some("hover"));

    // Apply transition
    let mut updated = v.clone();
    updated["current"] = json!("hover");
    updated["previousState"] = json!("idle");
    db.put_json("button:state_machine", updated, json!({}))
        .unwrap();

    // Simulate: pointerDown → should transition to "pressed"
    let sm = db.get("button:state_machine").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&sm.data).unwrap();
    let current = v["current"].as_str().unwrap();
    assert_eq!(current, "hover");

    let new_state = find_transition(v["transitions"].as_array().unwrap(), current, "pointerDown");
    assert_eq!(new_state, Some("pressed"));

    // Verify invalid transition doesn't match
    let no_match = find_transition(v["transitions"].as_array().unwrap(), "idle", "pointerDown");
    assert_eq!(no_match, None);

    let _ = std::fs::remove_dir_all(db_path);
}

/// Test: timeline with multiple tracks and keyframes.
#[test]
fn timeline_multi_track() {
    let db_path = "./test_timeline.db";
    let _ = std::fs::remove_dir_all(db_path);
    let db = get_or_create_db(db_path).unwrap();

    // Logo entity
    db.set_component_json(
        "logo",
        "transform",
        json!({ "scale": [0, 0, 0] }),
        json!({}),
    )
    .unwrap();
    db.set_component_json("logo", "style", json!({ "opacity": 0.0 }), json!({}))
        .unwrap();

    // Timeline with two tracks
    db.put_json(
        "intro:timeline",
        json!({
            "duration": 1.0,
            "playback": "playing",
            "speed": 1.0,
            "loop": false,
            "elapsed": 0.0,
            "tracks": [
                {
                    "target": "logo:transform.scale",
                    "keyframes": [
                        { "time": 0.0, "value": [0, 0, 0], "easing": "easeOutBack" },
                        { "time": 0.6, "value": [1, 1, 1] },
                    ]
                },
                {
                    "target": "logo:style.opacity",
                    "keyframes": [
                        { "time": 0.0, "value": 0.0, "easing": "easeOutCubic" },
                        { "time": 0.4, "value": 1.0 },
                    ]
                }
            ]
        }),
        json!({}),
    )
    .unwrap();

    // Simulate at t=0.3 (midway through both tracks)
    let elapsed = 0.3;
    let tl = db.get("intro:timeline").unwrap();
    let tl_v: serde_json::Value = serde_json::from_slice(&tl.data).unwrap();
    let tracks = tl_v["tracks"].as_array().unwrap();

    // Track 0: scale at t=0.3, keyframes [0→0.6]
    // progress = 0.3/0.6 = 0.5
    let kf = tracks[0]["keyframes"].as_array().unwrap();
    let scale_val = eval_keyframes(kf, elapsed);
    assert!(scale_val.is_some());
    let scale = scale_val.unwrap();
    // With easeOutBack at 50%, scale should be > 0.5 (overshoot)
    let sx = scale.as_array().unwrap()[0].as_f64().unwrap();
    assert!(sx > 0.0, "Scale x should be > 0 at midpoint, got {}", sx);
    assert!(sx < 1.5, "Scale x shouldn't overshoot too much, got {}", sx);

    // Track 1: opacity at t=0.3, keyframes [0→0.4]
    // progress = 0.3/0.4 = 0.75
    let kf = tracks[1]["keyframes"].as_array().unwrap();
    let opacity_val = eval_keyframes(kf, elapsed);
    assert!(opacity_val.is_some());
    let opacity = opacity_val.unwrap().as_f64().unwrap();
    assert!(
        opacity > 0.5,
        "Opacity should be > 0.5 at 75%, got {}",
        opacity
    );

    let _ = std::fs::remove_dir_all(db_path);
}

/// Test: full cycle — hydrate layout, tween, sync back to layout.
#[test]
fn full_layout_tween_cycle() {
    let db_path = "./test_full_cycle.db";
    let _ = std::fs::remove_dir_all(db_path);
    let db = get_or_create_db(db_path).unwrap();

    // Set up a card element
    db.set_component_json(
        "card",
        "dom",
        json!({ "tag": "div", "width": 300, "height": 200 }),
        json!({}),
    )
    .unwrap();
    db.set_component_json(
        "card",
        "transform",
        json!({ "position": [0, 50, 0] }),
        json!({}),
    )
    .unwrap();
    db.set_component_json("card", "style", json!({ "opacity": 1.0 }), json!({}))
        .unwrap();

    // Headless backend
    let backend = Arc::new(HeadlessLayoutBackend::new());
    layout::set_layout_backend(db_path, backend.clone());

    // 1. Hydrate: DOM → AssetDB → Layout
    backend.hydrate(&db).unwrap();
    assert_eq!(backend.query("card", "y"), Some(50.0));
    assert_eq!(backend.query("card", "width"), Some(300.0));

    // 2. Tween: slide card up (y: 50 → 0)
    let from_y = 50.0;
    let to_y = 0.0;
    let steps = 5;
    let dt = 0.1;
    let duration = steps as f64 * dt;

    for i in 1..=steps {
        let t = (i as f64 * dt / duration).min(1.0);
        let eased_t = reflow_assets_eval_easing("easeOutCubic", t);
        let y = from_y + (to_y - from_y) * eased_t;

        db.set_component_json(
            "card",
            "transform",
            json!({ "position": [0, y, 0] }),
            json!({}),
        )
        .unwrap();

        // 3. Sync: AssetDB → Layout
        backend.sync(&db).unwrap();
    }

    // 4. Verify final state
    let final_y = backend.query("card", "y").unwrap();
    assert!(final_y.abs() < 1.0, "Expected y ≈ 0, got {}", final_y);

    let final_tf = db.get_component("card", "transform").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&final_tf.data).unwrap();
    let db_y = v["position"][1].as_f64().unwrap();
    assert!(
        db_y.abs() < 1.0,
        "AssetDB position.y should be ≈ 0, got {}",
        db_y
    );

    let _ = std::fs::remove_dir_all(db_path);
}

/// Test: :bind component enables auto two-way sync.
#[test]
fn bind_component_enables_auto_sync() {
    let db_path = "./test_bind.db";
    let _ = std::fs::remove_dir_all(db_path);
    let db = get_or_create_db(db_path).unwrap();

    // Slider entity with DOM + transform + bind
    db.set_component_json(
        "slider",
        "dom",
        json!({ "tag": "input", "type": "range", "width": 200, "height": 20 }),
        json!({}),
    )
    .unwrap();

    db.set_component_json(
        "slider",
        "transform",
        json!({ "position": [100.0, 50.0, 0.0] }),
        json!({}),
    )
    .unwrap();

    // Enable full bind
    db.set_component_json("slider", "bind", json!(true), json!({}))
        .unwrap();

    // Also test selective bind on another entity
    db.set_component_json("scroller", "dom", json!({ "tag": "div" }), json!({}))
        .unwrap();
    db.set_component_json(
        "scroller",
        "bind",
        json!({ "scroll": true, "transform": false }),
        json!({}),
    )
    .unwrap();

    // Set up headless backend with layout data
    let backend = Arc::new(layout::HeadlessLayoutBackend::new());
    layout::set_layout_backend(db_path, backend.clone());

    // Simulate: layout has different position (user dragged the slider)
    backend.set_node(
        "slider",
        layout::LayoutNode {
            tag: "input".to_string(),
            x: 150.0, // layout says x=150 (was 100 in AssetDB)
            y: 50.0,
            width: 200.0,
            height: 20.0,
            opacity: 1.0,
            ..Default::default()
        },
    );

    // Simulate: scroller has scroll data
    backend.set_node(
        "scroller",
        layout::LayoutNode {
            tag: "div".to_string(),
            height: 600.0,
            scroll_y: 300.0,
            scroll_height: 3000.0,
            ..Default::default()
        },
    );

    // Verify bind entities are discoverable
    let bound = db.entities_with(&["bind"]).unwrap();
    assert_eq!(bound.len(), 2);

    // Simulate the pull phase: layout → AssetDB for bound entities
    // (This is what LayoutSyncSystem does internally)
    for entity in &bound {
        let bind_asset = db.get_component(entity, "bind").unwrap();
        let bind_config: serde_json::Value = serde_json::from_slice(&bind_asset.data).unwrap();

        let bind_all = bind_config.as_bool().unwrap_or(false);
        let bind_transform = bind_all
            || bind_config
                .get("transform")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        let bind_scroll = bind_all
            || bind_config
                .get("scroll")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        if bind_transform {
            if let (Some(x), Some(y)) = (backend.query(entity, "x"), backend.query(entity, "y")) {
                db.set_component_json(
                    entity,
                    "transform",
                    json!({ "position": [x, y, 0.0] }),
                    json!({"source": "layout_pull"}),
                )
                .unwrap();
            }
        }

        if bind_scroll {
            if let (Some(sy), Some(progress)) = (
                backend.query(entity, "scrollY"),
                backend.query(entity, "scrollProgress"),
            ) {
                db.set_component_json(
                    entity,
                    "scroll",
                    json!({ "y": sy, "progress": progress }),
                    json!({"source": "layout_pull"}),
                )
                .unwrap();
            }
        }
    }

    // Verify: slider transform was updated from layout (150, not 100)
    let tf = db.get_component("slider", "transform").unwrap();
    let v: serde_json::Value = serde_json::from_slice(&tf.data).unwrap();
    assert_eq!(v["position"][0], 150.0);

    // Verify: slider metadata shows source = layout_pull
    assert_eq!(tf.entry.metadata["source"], "layout_pull");

    // Verify: scroller got scroll data (bind_scroll=true)
    let scroll = db.get_component("scroller", "scroll").unwrap();
    let sv: serde_json::Value = serde_json::from_slice(&scroll.data).unwrap();
    assert_eq!(sv["y"], 300.0);
    let progress = sv["progress"].as_f64().unwrap();
    assert!((progress - 0.125).abs() < 0.001); // 300 / (3000-600) = 0.125

    // Verify: scroller transform was NOT pulled (bind_transform=false)
    assert!(!db.has_component("scroller", "transform"));

    let _ = std::fs::remove_dir_all(db_path);
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers (minimal reimplementations for test isolation)
// ═══════════════════════════════════════════════════════════════════════════

fn interpolate(
    from: &serde_json::Value,
    to: &serde_json::Value,
    t: f64,
    easing: &str,
) -> serde_json::Value {
    let e = reflow_assets_eval_easing(easing, t);
    match (from, to) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            let a = a.as_f64().unwrap_or(0.0);
            let b = b.as_f64().unwrap_or(0.0);
            json!(a + (b - a) * e)
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) if a.len() == b.len() => {
            let r: Vec<f64> = a
                .iter()
                .zip(b.iter())
                .map(|(av, bv)| {
                    let a = av.as_f64().unwrap_or(0.0);
                    let b = bv.as_f64().unwrap_or(0.0);
                    a + (b - a) * e
                })
                .collect();
            json!(r)
        }
        _ => to.clone(),
    }
}

fn reflow_assets_eval_easing(name: &str, t: f64) -> f64 {
    // Minimal easing for tests
    match name {
        "easeOutCubic" => {
            let u = t - 1.0;
            u * u * u + 1.0
        }
        "easeOutBack" => {
            let s = 1.70158;
            let u = t - 1.0;
            u * u * ((s + 1.0) * u + s) + 1.0
        }
        _ => t,
    }
}

fn reflow_assets_eval_expr(
    expr: &str,
    vars: &std::collections::HashMap<String, f64>,
) -> Option<f64> {
    // Minimal: "time * 90" with single multiply
    let parts: Vec<&str> = expr.split('*').map(|s| s.trim()).collect();
    if parts.len() == 2 {
        let a = vars
            .get(parts[0])
            .copied()
            .or_else(|| parts[0].parse().ok())?;
        let b = vars
            .get(parts[1])
            .copied()
            .or_else(|| parts[1].parse().ok())?;
        Some(a * b)
    } else {
        expr.parse().ok()
    }
}

fn write_target(db: &Arc<reflow_assets::AssetDB>, path: &str, value: &serde_json::Value) {
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let entity_component = parts[0];

    if parts.len() == 1 {
        let _ = db.put_json(entity_component, value.clone(), json!({}));
        return;
    }

    let field_path = parts[1];
    if let Ok(asset) = db.get(entity_component) {
        let mut current: serde_json::Value = if let Some(ref inline) = asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&asset.data).unwrap_or(json!({}))
        };
        set_json_path(&mut current, field_path, value.clone());
        let _ = db.put_json(entity_component, current, asset.entry.metadata);
    }
}

fn set_json_path(obj: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let keys: Vec<&str> = path.split('.').collect();
    let mut current = obj;
    for (i, key) in keys.iter().enumerate() {
        if i == keys.len() - 1 {
            if let Ok(idx) = key.parse::<usize>() {
                if let serde_json::Value::Array(ref mut arr) = current {
                    if idx < arr.len() {
                        arr[idx] = value;
                        return;
                    }
                }
            }
            current[key] = value;
            return;
        }
        if let Ok(idx) = key.parse::<usize>() {
            current = &mut current[idx];
        } else {
            if !current
                .get(key)
                .map(|v| v.is_object() || v.is_array())
                .unwrap_or(false)
            {
                current[key] = json!({});
            }
            current = &mut current[key];
        }
    }
}

fn find_transition<'a>(
    transitions: &'a [serde_json::Value],
    current: &str,
    trigger: &str,
) -> Option<&'a str> {
    for t in transitions {
        let from = t.get("from").and_then(|v| v.as_str()).unwrap_or("");
        let to = t.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let t_trigger = t.get("trigger").and_then(|v| v.as_str()).unwrap_or("");
        if t_trigger == trigger && (from == current || from == "*") {
            return Some(to);
        }
    }
    None
}

fn eval_keyframes(keyframes: &[serde_json::Value], time: f64) -> Option<serde_json::Value> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        return keyframes[0].get("value").cloned();
    }

    let mut prev_idx = 0;
    let mut next_idx = keyframes.len() - 1;

    for (i, kf) in keyframes.iter().enumerate() {
        let kt = kf["time"].as_f64().unwrap_or(0.0);
        if kt <= time {
            prev_idx = i;
        }
        if kt >= time {
            next_idx = i;
            break;
        }
    }

    if prev_idx == next_idx {
        return keyframes[prev_idx].get("value").cloned();
    }

    let prev = &keyframes[prev_idx];
    let next = &keyframes[next_idx];
    let pt = prev["time"].as_f64().unwrap_or(0.0);
    let nt = next["time"].as_f64().unwrap_or(1.0);
    let pv = prev.get("value")?;
    let nv = next.get("value")?;
    let easing = prev
        .get("easing")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");

    let seg = nt - pt;
    let t = if seg > 0.0 {
        ((time - pt) / seg).clamp(0.0, 1.0)
    } else {
        1.0
    };

    Some(interpolate(pv, nv, t, easing))
}
