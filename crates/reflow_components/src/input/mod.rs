//! System/browser input event actors.
//!
//! These are source actors that output event data when the runtime
//! delivers input events. On Wasm, the browser bridge injects events
//! via the actor's config. On native, the server injects events from
//! system APIs (winit, gilrs, etc.).
//!
//! The actors don't poll — they're triggered by the runtime when
//! an event occurs, with event data in their config.

mod keyboard;
mod mouse;
mod gamepad;
mod touch;
mod window;

pub use keyboard::KeyboardInputActor;
pub use mouse::MouseInputActor;
pub use gamepad::GamepadInputActor;
pub use touch::TouchInputActor;
pub use window::WindowEventActor;
