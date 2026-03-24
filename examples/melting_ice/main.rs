//! # Melting Ice Cube — SDF Ray March + Shader Graph Material
//!
//! SDF ray marcher = the real-time shader. No mesh conversion needed.
//! Shape and material evaluated per-pixel in one GPU compute pass.
//!
//! ```text
//! Phase 1 (material): NoiseTexture → ColorMix → BSDF → Compiler → shade WGSL
//! Phase 2 (geometry):  SdfBox → Round → Displace ─┐
//!                      SdfPuddle → Translate ──────┤→ Union → SdfScene(+shade) → SdfRender
//! Phase 3 (animate):   IntervalTrigger → AnimTime → SdfRender.time → per-frame output
//! ```

use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};
use std::collections::HashMap;

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg { Some(map.into_iter().collect()) } else { None }
}
fn wire(fa: &str, fp: &str, ta: &str, tp: &str) -> Connector {
    Connector {
        from: ConnectionPoint { actor: fa.to_owned(), port: fp.to_owned(), ..Default::default() },
        to: ConnectionPoint { actor: ta.to_owned(), port: tp.to_owned(), ..Default::default() },
    }
}
fn iip(node: &str, port: &str, msg: Message) -> InitialPacket {
    InitialPacket { to: ConnectionPoint::new(node, port, Some(msg)) }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Melting Ice Cube — SDF + Shader Graph ===\n");

    let fps = 24u32;
    let duration = 3.0f64;
    let total_frames = (duration * fps as f64) as usize;
    let w = 480u32;
    let h = 480u32;

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        // SDF geometry
        "tpl_sdf_box", "tpl_sdf_puddle", "tpl_sdf_union",
        "tpl_sdf_translate", "tpl_sdf_round", "tpl_sdf_displace",
        "tpl_sdf_scene",
        // SDF renderer
        "tpl_sdf_render",
        // Shader graph (material)
        "tpl_shader_noise_texture", "tpl_shader_color_mix",
        "tpl_shader_principled_bsdf", "tpl_shader_material_output",
        "tpl_shader_compiler",
        // Timing
        "tpl_interval_trigger", "tpl_animation_time",
        // Compositing + video
        "tpl_gpu_2d_render", "tpl_render_frame_collector",
        "tpl_video_encoder", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ PHASE 1: SHADER GRAPH (material) — fires first ═══

    net.add_node("noise", "tpl_shader_noise_texture", config(json!({ "scale": 5.0 })))?;
    net.add_node("color_mix", "tpl_shader_color_mix", config(json!({
        "mode": "mix",
        "a": { "type": "constVec3", "c": [0.85, 0.92, 0.98] },  // light ice blue
        "b": { "type": "constVec3", "c": [0.6, 0.78, 0.9] },   // slightly deeper blue
    })))?;
    net.add_node("bsdf", "tpl_shader_principled_bsdf", config(json!({
        "metallic": { "type": "constFloat", "c": 0.0 },
        "roughness": { "type": "constFloat", "c": 0.08 },
        "emission": { "type": "constVec3", "c": [0.1, 0.15, 0.2] },
        "emission_strength": { "type": "constFloat", "c": 0.2 },
        "alpha": { "type": "constFloat", "c": 0.9 },
        "ior": { "type": "constFloat", "c": 1.31 },
    })))?;
    net.add_node("mat_out", "tpl_shader_material_output", None)?;
    net.add_node("compiler", "tpl_shader_compiler", None)?;

    // Wire shader graph
    net.add_connection(wire("noise", "shader", "color_mix", "fac"));
    net.add_connection(wire("color_mix", "shader", "bsdf", "base_color"));
    net.add_connection(wire("bsdf", "shader", "mat_out", "surface"));
    net.add_connection(wire("mat_out", "shader", "compiler", "shader"));

    // ═══ PHASE 2: SDF GEOMETRY — fires after material is compiled ═══

    net.add_node("ice_box", "tpl_sdf_box", config(json!({
        "sizeX": 0.5, "sizeY": 0.5, "sizeZ": 0.5,
    })))?;
    net.add_node("ice_round", "tpl_sdf_round", config(json!({ "radius": 0.15 })))?;
    net.add_node("ice_displace", "tpl_sdf_displace", config(json!({
        "frequency": 5.0, "amplitude": 0.03, "octaves": 3,
    })))?;

    net.add_node("puddle_shape", "tpl_sdf_puddle", config(json!({
        "radius": 1.8, "height": 0.006, "noiseFreq": 1.2, "noiseAmp": 0.6,
    })))?;
    net.add_node("puddle", "tpl_sdf_translate", config(json!({ "x": 0.0, "y": -0.52, "z": 0.0 })))?;

    net.add_node("shape_union", "tpl_sdf_union", None)?;

    // SDF scene: receives shade WGSL from shader graph + SDF from geometry
    net.add_node("sdf_scene", "tpl_sdf_scene", config(json!({
        "softShadows": true, "shadowK": 16.0, "ao": true, "ambient": 0.25,
    })))?;

    // Wire SDF geometry
    net.add_connection(wire("ice_box", "sdf", "ice_round", "sdf"));
    net.add_connection(wire("ice_round", "sdf", "ice_displace", "sdf"));
    net.add_connection(wire("puddle_shape", "sdf", "puddle", "sdf"));
    net.add_connection(wire("ice_displace", "sdf", "shape_union", "sdf_a"));
    net.add_connection(wire("puddle", "sdf", "shape_union", "sdf_b"));
    net.add_connection(wire("shape_union", "sdf", "sdf_scene", "sdf"));

    // Shader graph shade → SDF scene
    net.add_connection(wire("compiler", "shade", "sdf_scene", "shade"));

    // ═══ SDF RAY MARCH RENDERER ═══

    net.add_node("render", "tpl_sdf_render", config(json!({
        "width": w, "height": h,
        "maxSteps": 200, "fov": 60.0,
        "cameraPosX": 4.5, "cameraPosY": 3.5, "cameraPosZ": 4.5,
        "cameraTargetX": 0.0, "cameraTargetY": -0.2, "cameraTargetZ": 0.0,
        "softShadows": true, "shadowK": 16.0, "ao": true,
        "ambient": 0.3,
        "lightDir": [0.5, 0.8, -0.3],
        "background": [0.55, 0.58, 0.65],
    })))?;

    net.add_connection(wire("sdf_scene", "sdf", "render", "sdf"));

    // ═══ PHASE 3: PER-FRAME ANIMATION ═══

    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": 1000 / fps as u64,
        "maxExecutions": total_frames,
        "startImmediately": false,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;

    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "render", "time"));

    // Start tick after SDF scene is compiled (includes shade)
    net.add_connection(wire("sdf_scene", "stats", "tick", "start"));

    // ═══ COMPOSITING + VIDEO ═══

    net.add_node("composite", "tpl_gpu_2d_render", config(json!({
        "width": w, "height": h, "msaa": 1,
        "background": [0.0, 0.0, 0.0, 0.0],
        "shapes": [{ "type": "image", "bounds": [0, 0, w, h], "z": 0 }],
        "text": [{
            "content": "Reflow — Melting Ice",
            "x": w as f64 - 200.0, "y": h as f64 - 14.0,
            "size": 14.0, "color": [1.0, 1.0, 1.0, 0.5],
            "tracking": 0.5, "center": false,
            "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        }],
    })))?;
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 20000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "melting_ice.mp4" })))?;

    net.add_connection(wire("render", "output", "composite", "data"));
    net.add_connection(wire("composite", "image", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // ═══ BOOTSTRAP ═══
    // Material first, then geometry (so shade is cached before SDF arrives)
    net.add_initial(iip("noise", "scale", Message::Float(5.0)));
    // Geometry triggers — fire after material chain completes
    // Use compiler.material as signal to start geometry
    net.add_connection(wire("compiler", "material", "ice_box", "_trigger"));
    net.add_connection(wire("compiler", "material", "puddle_shape", "_trigger"));

    println!("Pipeline: ShaderGraph → shade → SdfScene ← SDF geometry → SdfRender");
    println!("  {}x{}, {}fps, {} frames\n", w, h, fps, total_frames);

    let event_rx = net.get_event_receiver();
    tokio::spawn(async move {
        while let Ok(evt) = event_rx.recv_async().await {
            use reflow_network::network::NetworkEvent;
            if let NetworkEvent::ActorFailed { actor_id, error, .. } = &evt {
                eprintln!("[FAIL] actor={} err={}", actor_id, error);
            }
        }
    });

    let start = std::time::Instant::now();
    net.start()?;

    let mp4 = std::path::Path::new("melting_ice.mp4");
    let timeout = std::time::Duration::from_secs(120);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4.exists() && mp4.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let e = start.elapsed();
        if e.as_secs() % 10 == 0 && e.as_secs() > 0 { println!("  {:.0}s...", e.as_secs_f64()); }
        if e > timeout { eprintln!("Timed out"); break; }
    }

    let t = start.elapsed();
    if mp4.exists() {
        let sz = std::fs::metadata(mp4)?.len();
        println!("Saved: melting_ice.mp4 ({} bytes, {:.1}s)", sz, t.as_secs_f64());
    }
    std::process::exit(0);
}
