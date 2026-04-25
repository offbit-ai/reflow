//! Reflow actor pack: GPU-backed renderers (wgpu).

use reflow_pack_sdk::{PackHost, reflow_pack};

const TEMPLATES: &[&str] = &[
    "tpl_sdf_live_render",
    "tpl_sdf_render",
    "tpl_sdf_marching_cubes",
    "tpl_mesh_to_sdf",
    "tpl_scene_render",
    "tpl_gpu_2d_render",
];

#[reflow_pack]
fn register(host: &mut PackHost) {
    for id in TEMPLATES {
        let tid = id.to_string();
        host.register(id, move || {
            reflow_components::get_actor_for_template(&tid).unwrap_or_else(|| {
                panic!("pack did not bundle template '{tid}' — feature mismatch")
            })
        });
    }
}
