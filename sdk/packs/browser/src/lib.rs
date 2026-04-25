//! Reflow actor pack: browser automation.
//!
//! Ships a single template — `tpl_browser_screencast` — backed by
//! [`reflow_components`]'s `BrowserScreencastActor` (chromiumoxide).

use reflow_pack_sdk::{PackHost, reflow_pack};

const TEMPLATES: &[&str] = &["tpl_browser_screencast"];

#[reflow_pack]
fn register(host: &mut PackHost) {
    register_templates(host, TEMPLATES);
}

fn register_templates(host: &mut PackHost, ids: &'static [&'static str]) {
    for id in ids {
        let tid = id.to_string();
        host.register(id, move || {
            reflow_components::get_actor_for_template(&tid).unwrap_or_else(|| {
                panic!("pack did not bundle template '{tid}' — feature mismatch")
            })
        });
    }
}
