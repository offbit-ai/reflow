//! Reflow actor pack: H.264 video encoding.

use reflow_pack_sdk::{PackHost, reflow_pack};

const TEMPLATES: &[&str] = &["tpl_video_encoder"];

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
