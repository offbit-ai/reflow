//! End-to-end test of the callback-actor ABI.
//!
//! Walks the full lifecycle: build a graph from C, register a callback
//! actor, wire it into the network, start, observe at least one event on
//! the event stream, shut down cleanly, ensure the drop hook fires.

use reflow_rt_capi::*;
use std::ffi::{CString, c_void};
use std::os::raw::c_char;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

static DROP_CALLS: AtomicU32 = AtomicU32::new(0);
static CALLBACK_CALLS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn my_callback(
    _user_data: *mut c_void,
    ctx: *mut rfl_actor_ctx,
) -> rfl_status {
    CALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);

    let port = CString::new("in").unwrap();
    let out_port = CString::new("out").unwrap();

    // Pull the incoming Message JSON, wrap it as a new Flow message.
    let input_ptr = unsafe { rfl_ctx_input_json(ctx, port.as_ptr()) };
    if !input_ptr.is_null() {
        unsafe { rfl_string_free(input_ptr) };
    }

    let emit_msg = CString::new("{\"type\":\"Flow\"}").unwrap();
    unsafe { rfl_ctx_emit(ctx, out_port.as_ptr(), emit_msg.as_ptr()) }
}

unsafe extern "C" fn my_drop(_user_data: *mut c_void) {
    DROP_CALLS.fetch_add(1, Ordering::SeqCst);
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn callback_actor_roundtrip() {
    // Reset counters so repeated `cargo test` invocations don't stack.
    CALLBACK_CALLS.store(0, Ordering::SeqCst);
    DROP_CALLS.store(0, Ordering::SeqCst);

    unsafe {
        // ── register the actor ────────────────────────────────────────
        let component = cstr("my_tap");
        let inports = [cstr("in")];
        let outports = [cstr("out")];
        let inports_ptrs: Vec<*const c_char> = inports.iter().map(|s| s.as_ptr()).collect();
        let outports_ptrs: Vec<*const c_char> = outports.iter().map(|s| s.as_ptr()).collect();

        let actor = rfl_actor_new(
            component.as_ptr(),
            inports_ptrs.as_ptr(),
            1,
            outports_ptrs.as_ptr(),
            1,
            1, // await_all_inports
            Some(my_callback),
            ptr::null_mut(),
            Some(my_drop),
        );
        assert!(!actor.is_null(), "rfl_actor_new returned null");

        // ── build a network with two instances of the callback actor ──
        let net = rfl_network_new();
        assert!(!net.is_null());

        let template = cstr("tpl_my_tap");
        let status = rfl_network_register_actor(net, template.as_ptr(), actor);
        assert_eq!(status, rfl_status::Ok);

        let id_a = cstr("a");
        let id_b = cstr("b");
        assert_eq!(
            rfl_network_add_node(net, id_a.as_ptr(), template.as_ptr(), ptr::null()),
            rfl_status::Ok
        );
        assert_eq!(
            rfl_network_add_node(net, id_b.as_ptr(), template.as_ptr(), ptr::null()),
            rfl_status::Ok
        );

        let out_port = cstr("out");
        let in_port = cstr("in");
        assert_eq!(
            rfl_network_add_connection(
                net,
                id_a.as_ptr(),
                out_port.as_ptr(),
                id_b.as_ptr(),
                in_port.as_ptr(),
            ),
            rfl_status::Ok
        );

        // Seed A.in so the pipeline has something to do.
        let msg_json = cstr("{\"type\":\"Flow\"}");
        assert_eq!(
            rfl_network_add_initial(net, id_a.as_ptr(), in_port.as_ptr(), msg_json.as_ptr()),
            rfl_status::Ok
        );

        // ── subscribe to events, then start ───────────────────────────
        let events = rfl_network_events(net);
        assert!(!events.is_null());

        assert_eq!(rfl_network_start(net), rfl_status::Ok);

        // Drain a few events — we only require that *something* shows up.
        let mut saw_event = false;
        for _ in 0..20 {
            let mut out: *mut c_char = ptr::null_mut();
            match rfl_events_recv(events, 250, &mut out) {
                rfl_status::Ok => {
                    assert!(!out.is_null());
                    rfl_string_free(out);
                    saw_event = true;
                    // Give the callback a few more ticks to run.
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    if CALLBACK_CALLS.load(Ordering::SeqCst) > 0 {
                        break;
                    }
                }
                rfl_status::InvalidState => continue, // timeout
                other => panic!("unexpected event status: {:?}", other),
            }
        }
        assert!(saw_event, "no network events observed");

        // ── shutdown + cleanup ────────────────────────────────────────
        assert_eq!(rfl_network_shutdown(net), rfl_status::Ok);
        rfl_events_free(events);
        rfl_network_free(net);

        // Shut the runtime down first so pending actor tasks unwind and
        // release their Arc<dyn Actor> captures. Only then can the final
        // Arc<CapiCallback> drop and fire the user_data_drop hook.
        rfl_runtime_shutdown();
        for _ in 0..50 {
            if DROP_CALLS.load(Ordering::SeqCst) >= 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(
            CALLBACK_CALLS.load(Ordering::SeqCst) > 0,
            "callback never fired"
        );
        assert_eq!(
            DROP_CALLS.load(Ordering::SeqCst),
            1,
            "user_data_drop should fire exactly once, got {}",
            DROP_CALLS.load(Ordering::SeqCst)
        );
    }
}
