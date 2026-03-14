//! SDF codegen integration tests.

use reflow_sdf::codegen;
use reflow_sdf::ir::{SceneSettings, SdfNode};

#[test]
fn test_codegen_produces_valid_wgsl_structure() {
    let scene = SdfNode::smooth_union(
        SdfNode::sphere(1.0)
            .translate([0.8, 0.0, 0.0]),
        SdfNode::cube(0.7)
            .rotate([0.0, 45.0, 0.0]),
        0.3,
    )
    .into_scene_with(SceneSettings {
        width: 256,
        height: 256,
        ao: true,
        ..Default::default()
    });

    let compiled = codegen::compile(&scene);

    assert!(compiled.wgsl.contains("fn sdf_scene(p: vec3f) -> f32"));
    assert!(compiled.wgsl.contains("fn ray_march"));
    assert!(compiled.wgsl.contains("fn calc_ao"));
    assert!(compiled.wgsl.contains("@compute @workgroup_size(8, 8, 1)"));
    assert!(compiled.wgsl.contains("fn smin"));
    assert!(compiled.uses_smooth_ops);
    assert!(!compiled.uses_noise);

    let sdf_body = compiled.wgsl
        .split("fn sdf_scene(p: vec3f) -> f32 {")
        .nth(1).unwrap()
        .split('}').next().unwrap();
    assert!(!sdf_body.contains("let p ="), "Variable shadowing detected");
}

#[test]
fn test_complex_scene_with_noise_and_shadows() {
    let scene = SdfNode::union(
        SdfNode::plane([0.0, 1.0, 0.0], 1.0),
        SdfNode::smooth_union(
            SdfNode::torus(1.5, 0.3).twist(0.8).translate([0.0, 0.5, 0.0]),
            SdfNode::sphere(0.4).displace(5.0, 0.05, 3).mirror([1.0, 0.0, 1.0]),
            0.2,
        ),
    )
    .into_scene_with(SceneSettings {
        soft_shadows: true,
        ..Default::default()
    });

    let compiled = codegen::compile(&scene);
    assert!(compiled.uses_noise);
    assert!(compiled.uses_smooth_ops);
    assert!(compiled.wgsl.contains("fn soft_shadow"));
    assert!(compiled.node_count >= 5);
}
