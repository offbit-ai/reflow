//! # Flow-Wired Motion — Fan-in / Fan-out correctness test
//!
//! Same visual output as `motion_2d`, but shapes and keyframes are wired as
//! DAG flow data using proper actors rather than config:
//!
//! - **KeyframeActor** instances define tracks, fed into timeline via fan-in
//! - **Shape2DActor** instances produce shape geometry + rendering properties
//! - **AnimationTimeline** synchronizes all tracks atomically per tick
//!
//! ## Topology
//!
//! ```text
//! IIP → kf_s0_x (Keyframe) → track ──→ timeline:tracks  ┐
//! IIP → kf_s0_y (Keyframe) → track ──→ timeline:tracks  ├─ fan-in (14 tracks)
//! IIP → kf_s1_x (Keyframe) → track ──→ timeline:tracks  │
//! ...                                                     ┘
//!
//! IIP → shape_0 (Shape2D) → metadata ──→ renderer:primitives  ┐
//! IIP → shape_1 (Shape2D) → metadata ──→ renderer:primitives  ├─ fan-in (4 shapes)
//! ...                                                          ┘
//!
//! tick ──→ timeline:tick
//! timeline:values ──→ renderer:values  (atomic, synchronized)
//! renderer:image ──→ collector → encoder → save
//! ```

use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};
use std::collections::HashMap;

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg {
        Some(map.into_iter().collect())
    } else {
        None
    }
}
fn wire(fa: &str, fp: &str, ta: &str, tp: &str) -> Connector {
    Connector {
        from: ConnectionPoint {
            actor: fa.to_owned(),
            port: fp.to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: ta.to_owned(),
            port: tp.to_owned(),
            ..Default::default()
        },
    }
}
fn iip(node: &str, port: &str, msg: Message) -> InitialPacket {
    InitialPacket {
        to: ConnectionPoint::new(node, port, Some(msg)),
    }
}

/// Keyframe track definition
struct KfDef {
    name: &'static str,
    keyframes: Value,
    duration: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Flow-Wired Motion — Fan-in / Fan-out ===\n");

    let w = 800u32;
    let h = 450u32;
    let fps = 30u32;
    let dur = 8.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;

    println!("{}x{}, {}fps, {:.0}s = {} frames\n", w, h, fps, dur, frames);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_animation_timeline",
        "tpl_keyframe",
        "tpl_shape_2d",
        "tpl_gpu_2d_render",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TIMING
    // ═══════════════════════════════════════════════════════════════════════
    net.add_node(
        "tick",
        "tpl_interval_trigger",
        config(json!({
            "interval": ms,
            "maxExecutions": frames,
            "startImmediately": false,
        })),
    )?;
    net.add_node(
        "time",
        "tpl_animation_time",
        config(json!({ "fps": fps, "speed": 1.0 })),
    )?;

    // ═══════════════════════════════════════════════════════════════════════
    // KEYFRAMES — 14 KeyframeActors → timeline:tracks (FAN-IN)
    // Each actor emits its track definition (name + keyframes) once on
    // trigger. The timeline accumulates them and evaluates atomically.
    // ═══════════════════════════════════════════════════════════════════════

    let tracks: Vec<KfDef> = vec![
        // Shape 0: Orange rect — spiral, scale in/out, 2 full rotations
        KfDef {
            name: "s0_x",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cx, "easing": "easeInOutCubic"},
                {"time": 2.0, "value": cx + 150.0, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cx, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cx - 150.0, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cx}
            ]),
        },
        KfDef {
            name: "s0_y",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cy, "easing": "easeInOutCubic"},
                {"time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cy}
            ]),
        },
        KfDef {
            name: "s0_scale",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0, "easing": "easeOutBack"},
                {"time": 1.5, "value": 1.0},
                {"time": 6.5, "value": 1.0, "easing": "easeInCubic"},
                {"time": 8.0, "value": 0.0}
            ]),
        },
        KfDef {
            name: "s0_rotation",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0}, {"time": 8.0, "value": 720.0}
            ]),
        },
        // Shape 1: Blue circle — wide orbit
        KfDef {
            name: "s1_x",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cx + 250.0, "easing": "easeInOutSine"},
                {"time": 2.0, "value": cx, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cx - 250.0, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cx, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cx + 250.0}
            ]),
        },
        KfDef {
            name: "s1_y",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cy - 120.0, "easing": "easeInOutSine"},
                {"time": 2.0, "value": cy + 120.0, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cy - 120.0, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cy + 120.0, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cy - 120.0}
            ]),
        },
        KfDef {
            name: "s1_scale",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0},
                {"time": 0.5, "value": 0.0, "easing": "easeOutBack"},
                {"time": 1.5, "value": 1.0},
                {"time": 7.0, "value": 1.0, "easing": "easeInCubic"},
                {"time": 8.0, "value": 0.0}
            ]),
        },
        // Shape 2: Pink circle — counter-orbit
        KfDef {
            name: "s2_x",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cx - 200.0, "easing": "easeInOutSine"},
                {"time": 2.0, "value": cx + 200.0, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cx - 200.0, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cx + 200.0, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cx - 200.0}
            ]),
        },
        KfDef {
            name: "s2_y",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": cy + 100.0, "easing": "easeInOutSine"},
                {"time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine"},
                {"time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine"},
                {"time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine"},
                {"time": 8.0, "value": cy + 100.0}
            ]),
        },
        KfDef {
            name: "s2_scale",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0},
                {"time": 1.0, "value": 0.0, "easing": "easeOutBack"},
                {"time": 2.0, "value": 1.0},
                {"time": 6.5, "value": 1.0, "easing": "easeInCubic"},
                {"time": 8.0, "value": 0.0}
            ]),
        },
        // Shape 3: Purple hex border — pulses, slow counter-rotate
        KfDef {
            name: "s3_x",
            duration: dur,
            keyframes: json!([{"time": 0.0, "value": cx}, {"time": 8.0, "value": cx}]),
        },
        KfDef {
            name: "s3_y",
            duration: dur,
            keyframes: json!([{"time": 0.0, "value": cy}, {"time": 8.0, "value": cy}]),
        },
        KfDef {
            name: "s3_scale",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0},
                {"time": 2.0, "value": 0.0, "easing": "easeOutElastic"},
                {"time": 3.5, "value": 1.0},
                {"time": 4.5, "value": 0.85, "easing": "easeInOutSine"},
                {"time": 5.5, "value": 1.0, "easing": "easeInOutSine"},
                {"time": 6.5, "value": 0.85, "easing": "easeInCubic"},
                {"time": 8.0, "value": 0.0}
            ]),
        },
        KfDef {
            name: "s3_rotation",
            duration: dur,
            keyframes: json!([
                {"time": 0.0, "value": 0.0}, {"time": 8.0, "value": -180.0}
            ]),
        },
    ];

    // Create a KeyframeActor per track
    for track in &tracks {
        net.add_node(
            &format!("kf_{}", track.name),
            "tpl_keyframe",
            config(json!({
                "name": track.name,
                "keyframes": track.keyframes,
                "duration": track.duration,
            })),
        )?;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // TIMELINE — synchronization point. Receives tracks via flow fan-in,
    // evaluates atomically per tick, outputs merged values.
    // autoplay: false — waits for "play" control after data is loaded.
    // ═══════════════════════════════════════════════════════════════════════
    net.add_node(
        "tl",
        "tpl_animation_timeline",
        config(json!({
            "duration": dur,
            "autoplay": true,
            "dt": 1.0 / fps as f64,
        })),
    )?;

    // ═══════════════════════════════════════════════════════════════════════
    // SHAPES — 4 Shape2DActors → renderer:primitives (FAN-IN)
    // One-shot: IIP triggers shape output, renderer pools by index.
    // ═══════════════════════════════════════════════════════════════════════

    // Shape initial positions (cx/cy) match first keyframe positions so shapes
    // render correctly even before animation values arrive.
    net.add_node(
        "shape_0",
        "tpl_shape_2d",
        config(json!({
            "shape": "rect", "width": 100.0, "height": 100.0, "cornerRadius": 10.0,
            "cx": cx, "cy": cy, "index": 0,
            "color": [1.0, 0.53, 0.13, 1.0],
            "shadow": { "x": 0, "y": 4, "blur": 25, "color": [1.0, 0.4, 0.0, 0.5] },
        })),
    )?;
    net.add_node(
        "shape_1",
        "tpl_shape_2d",
        config(json!({
            "shape": "circle", "radius": 28.0,
            "cx": cx + 250.0, "cy": cy - 120.0, "index": 1,
            "color": [0.25, 0.75, 1.0, 0.9],
            "shadow": { "x": 0, "y": 2, "blur": 20, "color": [0.2, 0.6, 1.0, 0.4] },
        })),
    )?;
    net.add_node(
        "shape_2",
        "tpl_shape_2d",
        config(json!({
            "shape": "circle", "radius": 20.0,
            "cx": cx - 200.0, "cy": cy + 100.0, "index": 2,
            "color": [1.0, 0.25, 0.5, 0.85],
            "shadow": { "x": 0, "y": 2, "blur": 15, "color": [1.0, 0.2, 0.4, 0.3] },
        })),
    )?;
    net.add_node(
        "shape_3",
        "tpl_shape_2d",
        config(json!({
            "shape": "rect", "width": 140.0, "height": 140.0, "cornerRadius": 0.0,
            "cx": cx, "cy": cy, "index": 3,
            "color": [0.0, 0.0, 0.0, 0.0],
            "border": { "width": 2.0, "color": [0.5, 0.38, 1.0, 0.7] },
        })),
    )?;

    // ═══════════════════════════════════════════════════════════════════════
    // RENDERER — no config shapes, all data arrives via flow
    // ═══════════════════════════════════════════════════════════════════════
    net.add_node(
        "render",
        "tpl_gpu_2d_render",
        config(json!({
            "width": w, "height": h,
            "background": [0.02, 0.008, 0.06, 1.0],
        })),
    )?;

    // ═══════════════════════════════════════════════════════════════════════
    // VIDEO PIPELINE
    // ═══════════════════════════════════════════════════════════════════════
    net.add_node(
        "collector",
        "tpl_render_frame_collector",
        config(json!({
            "totalFrames": frames, "width": w, "height": h, "fps": fps,
        })),
    )?;
    net.add_node(
        "encoder",
        "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 8000 })),
    )?;
    net.add_node(
        "save",
        "tpl_file_save",
        config(json!({ "path": "motion_2d_flow.mp4" })),
    )?;

    // ═══════════════════════════════════════════════════════════════════════
    // WIRING
    // ═══════════════════════════════════════════════════════════════════════

    // Tick drives time and timeline
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "tl", "tick"));

    // Fan-in: 14 KeyframeActors → timeline:tracks (track definitions)
    for track in &tracks {
        net.add_connection(wire(
            &format!("kf_{}", track.name),
            "track",
            "tl",
            "tracks",
        ));
    }

    // Timeline → renderer (atomic synchronized values per tick)
    net.add_connection(wire("tl", "values", "render", "values"));

    // Fan-in: 4 Shape2DActors → renderer:primitives
    net.add_connection(wire("shape_0", "metadata", "render", "primitives"));
    net.add_connection(wire("shape_1", "metadata", "render", "primitives"));
    net.add_connection(wire("shape_2", "metadata", "render", "primitives"));
    net.add_connection(wire("shape_3", "metadata", "render", "primitives"));

    // Video pipeline
    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap: trigger KeyframeActors to emit track definitions (IIPs processed first)
    for track in &tracks {
        net.add_initial(iip(&format!("kf_{}", track.name), "trigger", Message::Flow));
    }
    // Trigger shape actors
    net.add_initial(iip("shape_0", "params", Message::Flow));
    net.add_initial(iip("shape_1", "params", Message::Flow));
    net.add_initial(iip("shape_2", "params", Message::Flow));
    net.add_initial(iip("shape_3", "params", Message::Flow));
    // Start tick loop — startImmediately:false delays first tick by one
    // interval (33ms), giving IIPs time to deliver tracks and shapes.
    net.add_initial(iip("tick", "start", Message::Flow));

    let n_kf = tracks.len();
    println!("DAG topology:");
    println!("  {} KeyframeActors ──fan-in──→ timeline:tracks", n_kf);
    println!("  timeline:values ──→ renderer:values (synchronized)");
    println!("  4 Shape2DActors ──fan-in──→ renderer:primitives");
    println!("  tick ──→ timeline:tick + time");
    println!("  renderer → collector → encoder → save");
    println!("\nRunning...\n");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("motion_2d_flow.mp4");
    let timeout = std::time::Duration::from_secs(120);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let e = start.elapsed();
        if e.as_secs().is_multiple_of(10) && e.as_secs() > 0 {
            println!("  {:.0}s...", e.as_secs());
        }
        if e > timeout {
            eprintln!("Timed out");
            break;
        }
    }

    net.shutdown();
    let total_time = start.elapsed();

    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("\nSaved: motion_2d_flow.mp4 ({} bytes)", size);
        println!(
            "Total: {:.1}s ({:.1} effective fps)",
            total_time.as_secs_f64(),
            frames as f64 / total_time.as_secs_f64()
        );
    }
    println!("Done!");
    Ok(())
}
