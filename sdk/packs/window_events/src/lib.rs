//! Reflow actor pack: window / input event sources.

use reflow_pack_sdk::{PackHost, reflow_pack};

const TEMPLATES: &[&str] = &[
    "tpl_keyboard_input",
    "tpl_mouse_input",
    "tpl_gamepad_input",
    "tpl_touch_input",
    "tpl_window_event",
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
