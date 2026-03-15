//! System/browser input event actors.
//!
//! Source actors triggered by the runtime when input events occur.
//! The runtime sends event data as `Message::Object` via
//! `network.send_to_actor(actor_id, "_event", event_data)`.
//!
//! Feature flags:
//! - `window-events`: keyboard, mouse, touch, window actors (winit on native)
//! - `browser-events`: all of the above + browser-specific APIs (web_sys on Wasm)

mod gamepad;
mod keyboard;
mod mouse;
mod touch;
mod window;

pub use gamepad::GamepadInputActor;
pub use keyboard::KeyboardInputActor;
pub use mouse::MouseInputActor;
pub use touch::TouchInputActor;
pub use window::WindowEventActor;

use reflow_actor::ActorContext;

/// Extract event data from the first Object message in the payload.
/// Falls back to config hashmap for static/testing scenarios.
pub(crate) fn extract_event_data(ctx: &ActorContext) -> serde_json::Value {
    let payload = ctx.get_payload();
    for (_port, msg) in payload.iter() {
        if let crate::Message::Object(obj) = msg {
            return obj.as_ref().clone().into();
        }
    }
    let config = ctx.get_config_hashmap();
    serde_json::to_value(config).unwrap_or_default()
}
