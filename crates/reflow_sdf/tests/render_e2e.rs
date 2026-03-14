//! End-to-end test: build SDF IR → compile to WGSL → render via wgpu → save PNG.
//!
//! This test requires a GPU (Metal on macOS, Vulkan on Linux).
//! Run with: cargo test -p reflow_sdf --test render_e2e -- --nocapture

use reflow_sdf::codegen;
use reflow_sdf::ir::{SceneSettings, SdfNode};

#[test]
fn test_codegen_produces_valid_wgsl_structure() {
    let scene = SdfNode::smooth_union(
        SdfNode::sphere(1.0)
            .translate([0.8, 0.0, 0.0])
            .with_color(0.9, 0.2, 0.1),
        SdfNode::cube(0.7)
            .rotate([0.0, 45.0, 0.0])
            .with_color(0.1, 0.5, 0.9),
        0.3,
    )
    .into_scene_with(SceneSettings {
        width: 256,
        height: 256,
        max_steps: 64,
        ao: true,
        soft_shadows: false,
        ..Default::default()
    });

    let compiled = codegen::compile(&scene);

    // Verify shader structure
    assert!(compiled.wgsl.contains("struct Uniforms"), "Missing Uniforms struct");
    assert!(compiled.wgsl.contains("fn sdf_scene(p: vec3f) -> f32"), "Missing sdf_scene fn");
    assert!(compiled.wgsl.contains("fn ray_march"), "Missing ray_march fn");
    assert!(compiled.wgsl.contains("fn shade"), "Missing shade fn");
    assert!(compiled.wgsl.contains("fn calc_normal"), "Missing calc_normal fn");
    assert!(compiled.wgsl.contains("fn calc_ao"), "Missing calc_ao fn");
    assert!(compiled.wgsl.contains("@compute @workgroup_size(8, 8, 1)"), "Missing compute entry");
    assert!(compiled.wgsl.contains("fn smin"), "Missing smooth min");
    assert!(compiled.uses_smooth_ops);
    assert!(!compiled.uses_noise);

    // Verify no variable shadowing
    let sdf_body = compiled.wgsl
        .split("fn sdf_scene(p: vec3f) -> f32 {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!sdf_body.contains("let p ="), "Variable shadowing detected in sdf_scene");

    println!("Shader size: {} bytes", compiled.wgsl.len());
    println!("Node count: {}", compiled.node_count);
    println!("\n--- Generated WGSL (sdf_scene body) ---");
    println!("{}", sdf_body.trim());
}

#[test]
fn test_complex_scene_codegen() {
    // A more complex scene: ground plane + twisted torus + mirrored displaced spheres
    let ground = SdfNode::plane([0.0, 1.0, 0.0], 1.0);
    let torus = SdfNode::torus(1.5, 0.3)
        .twist(0.8)
        .translate([0.0, 0.5, 0.0])
        .with_color(0.2, 0.8, 0.3);
    let spheres = SdfNode::sphere(0.4)
        .displace(5.0, 0.05, 3)
        .mirror([1.0, 0.0, 1.0])
        .translate([2.0, 0.5, 0.0]);

    let scene = SdfNode::union(
        ground,
        SdfNode::smooth_union(torus, spheres, 0.2),
    )
    .into_scene_with(SceneSettings {
        width: 512,
        height: 512,
        max_steps: 128,
        ao: true,
        soft_shadows: true,
        camera_pos: [4.0, 3.0, 5.0],
        ..Default::default()
    });

    let compiled = codegen::compile(&scene);
    assert!(compiled.uses_noise, "Should use noise for displacement");
    assert!(compiled.uses_smooth_ops, "Should use smooth ops");
    assert!(compiled.wgsl.contains("fn soft_shadow"), "Should include soft shadows");
    assert!(compiled.node_count >= 5, "Should have at least 5 nodes");

    println!("Complex scene: {} bytes, {} nodes", compiled.wgsl.len(), compiled.node_count);
}
