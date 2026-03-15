//! Browser event binding for Wasm.
//!
//! Sets up DOM event listeners that route browser events to input actors
//! in the network via `inject_input_event`.
//!
//! Usage from JavaScript:
//! ```js
//! import { bindInputEvents, unbindInputEvents } from 'reflow';
//! const cleanup = bindInputEvents(network, document.body);
//! // later: cleanup();
//! ```

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Bind browser input events to a Reflow network.
///
/// Attaches keyboard, mouse, touch, and resize listeners to `target`
/// (typically `document` or a canvas element). Events are routed to
/// the matching input actor type via `network.injectInputEvent()`.
///
/// Returns a cleanup closure that removes all listeners.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = bindInputEvents)]
pub fn bind_input_events(
    network: &crate::network::GraphNetwork,
    target: web_sys::EventTarget,
) -> Result<js_sys::Function, JsValue> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let closures: Rc<RefCell<Vec<(String, Closure<dyn FnMut(web_sys::Event)>)>>> =
        Rc::new(RefCell::new(Vec::new()));

    let net = network.clone();

    // Helper to add a listener and store the closure for cleanup
    macro_rules! add_listener {
        ($event:expr, $component:expr, $extract:expr) => {
            let net_clone = net.clone();
            let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
                let data = $extract(&event);
                let _ = net_clone.inject_input_event($component, data);
            });
            target
                .add_event_listener_with_callback($event, closure.as_ref().unchecked_ref())
                .map_err(|e| JsValue::from_str(&format!("Failed to add {} listener: {:?}", $event, e)))?;
            closures.borrow_mut().push(($event.to_string(), closure));
        };
    }

    // Keyboard events
    add_listener!("keydown", "tpl_keyboard_input", |e: &web_sys::Event| {
        let ke = e.dyn_ref::<web_sys::KeyboardEvent>().unwrap();
        serde_wasm_bindgen::to_value(&serde_json::json!({
            "type": "keydown",
            "key": ke.key(),
            "code": ke.code(),
            "altKey": ke.alt_key(),
            "ctrlKey": ke.ctrl_key(),
            "shiftKey": ke.shift_key(),
            "metaKey": ke.meta_key(),
            "repeat": ke.repeat(),
        })).unwrap_or(JsValue::NULL)
    });

    add_listener!("keyup", "tpl_keyboard_input", |e: &web_sys::Event| {
        let ke = e.dyn_ref::<web_sys::KeyboardEvent>().unwrap();
        serde_wasm_bindgen::to_value(&serde_json::json!({
            "type": "keyup",
            "key": ke.key(),
            "code": ke.code(),
            "altKey": ke.alt_key(),
            "ctrlKey": ke.ctrl_key(),
            "shiftKey": ke.shift_key(),
            "metaKey": ke.meta_key(),
            "repeat": ke.repeat(),
        })).unwrap_or(JsValue::NULL)
    });

    // Mouse events
    for event_name in &["mousedown", "mouseup", "mousemove", "click", "dblclick", "contextmenu"] {
        let name = event_name.to_string();
        let net_clone = net.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let me = event.dyn_ref::<web_sys::MouseEvent>().unwrap();
            let data = serde_wasm_bindgen::to_value(&serde_json::json!({
                "type": name,
                "x": me.offset_x(),
                "y": me.offset_y(),
                "clientX": me.client_x(),
                "clientY": me.client_y(),
                "button": me.button(),
                "buttons": me.buttons(),
                "movementX": me.movement_x(),
                "movementY": me.movement_y(),
            })).unwrap_or(JsValue::NULL);
            let _ = net_clone.inject_input_event("tpl_mouse_input", data);
        });
        target
            .add_event_listener_with_callback(event_name, closure.as_ref().unchecked_ref())
            .map_err(|e| JsValue::from_str(&format!("Failed to add {} listener: {:?}", event_name, e)))?;
        closures.borrow_mut().push((event_name.to_string(), closure));
    }

    // Wheel event
    {
        let net_clone = net.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let we = event.dyn_ref::<web_sys::WheelEvent>().unwrap();
            let data = serde_wasm_bindgen::to_value(&serde_json::json!({
                "type": "wheel",
                "deltaX": we.delta_x(),
                "deltaY": we.delta_y(),
                "x": we.offset_x(),
                "y": we.offset_y(),
            })).unwrap_or(JsValue::NULL);
            let _ = net_clone.inject_input_event("tpl_mouse_input", data);
        });
        target
            .add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref())?;
        closures.borrow_mut().push(("wheel".to_string(), closure));
    }

    // Touch events
    for event_name in &["touchstart", "touchmove", "touchend", "touchcancel"] {
        let name = event_name.to_string();
        let net_clone = net.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let te = event.dyn_ref::<web_sys::TouchEvent>().unwrap();
            let touch_list = te.touches();
            let mut touches = Vec::new();
            for i in 0..touch_list.length() {
                if let Some(touch) = touch_list.get(i) {
                    touches.push(serde_json::json!({
                        "id": touch.identifier(),
                        "x": touch.client_x(),
                        "y": touch.client_y(),
                        "force": touch.force(),
                        "radiusX": touch.radius_x(),
                        "radiusY": touch.radius_y(),
                    }));
                }
            }
            let data = serde_wasm_bindgen::to_value(&serde_json::json!({
                "type": name,
                "touches": touches,
            })).unwrap_or(JsValue::NULL);
            let _ = net_clone.inject_input_event("tpl_touch_input", data);
        });
        target
            .add_event_listener_with_callback(event_name, closure.as_ref().unchecked_ref())?;
        closures.borrow_mut().push((event_name.to_string(), closure));
    }

    // Window resize (on window, not target)
    {
        let net_clone = net.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            let window = web_sys::window().unwrap();
            let data = serde_wasm_bindgen::to_value(&serde_json::json!({
                "type": "resize",
                "width": window.inner_width().unwrap().as_f64().unwrap_or(0.0),
                "height": window.inner_height().unwrap().as_f64().unwrap_or(0.0),
                "devicePixelRatio": window.device_pixel_ratio(),
            })).unwrap_or(JsValue::NULL);
            let _ = net_clone.inject_input_event("tpl_window_event", data);
        });
        let window = web_sys::window().unwrap();
        window
            .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())?;
        closures.borrow_mut().push(("resize".to_string(), closure));
    }

    // Build cleanup function
    let target_clone = target.clone();
    let cleanup = Closure::<dyn FnMut()>::new(move || {
        for (event_name, closure) in closures.borrow().iter() {
            if event_name == "resize" {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        event_name,
                        closure.as_ref().unchecked_ref(),
                    );
                }
            } else {
                let _ = target_clone.remove_event_listener_with_callback(
                    event_name,
                    closure.as_ref().unchecked_ref(),
                );
            }
        }
    });

    let js_cleanup = cleanup.as_ref().unchecked_ref::<js_sys::Function>().clone();
    cleanup.forget(); // prevent drop — JS holds the reference

    Ok(js_cleanup)
}

// Native stub — event injection is handled by the server/runtime
#[cfg(not(target_arch = "wasm32"))]
pub fn bind_input_events_native(
    _network: &crate::network::Network,
) {
    // On native, the runtime (winit event loop, server, etc.)
    // calls network.inject_input_event() directly.
}
