//! Smoke tests for the typed message / stream / subgraph-builder ABI.

use reflow_rt_capi::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn message_constructors_and_accessors() {
    unsafe {
        // Integer round-trip.
        let m = rfl_message_integer(42);
        assert_eq!(rfl_message_get_kind(m), rfl_message_kind::Integer);
        let mut out_i: i64 = 0;
        assert_eq!(rfl_message_as_integer(m, &mut out_i), 1);
        assert_eq!(out_i, 42);
        rfl_message_free(m);

        // String round-trip.
        let s = cstr("hello");
        let m = rfl_message_string(s.as_ptr());
        let got = rfl_message_as_string(m);
        assert!(!got.is_null());
        let str_back = CStr::from_ptr(got).to_str().unwrap().to_owned();
        rfl_string_free(got);
        assert_eq!(str_back, "hello");
        rfl_message_free(m);

        // Bytes borrow.
        let data = [1u8, 2, 3, 4];
        let m = rfl_message_bytes(data.as_ptr(), data.len());
        let mut p: *const u8 = std::ptr::null();
        let mut n: usize = 0;
        assert_eq!(rfl_message_bytes_borrow(m, &mut p, &mut n), 1);
        assert_eq!(n, 4);
        assert_eq!(std::slice::from_raw_parts(p, n), &data);
        rfl_message_free(m);

        // JSON fallback.
        let j = cstr("{\"type\":\"Flow\"}");
        let m = rfl_message_from_json(j.as_ptr());
        assert_eq!(rfl_message_get_kind(m), rfl_message_kind::Flow);
        rfl_message_free(m);
    }
}

#[test]
fn stream_producer_consumer_roundtrip() {
    unsafe {
        // Spin up a producer stream.
        let origin = cstr("a");
        let port = cstr("out");
        let s = rfl_stream_new(32, origin.as_ptr(), port.as_ptr(), std::ptr::null());
        assert!(!s.is_null());

        // Send two Data frames, then End.
        let buf1 = [1u8, 2, 3];
        let buf2 = [9u8, 8, 7, 6];
        assert_eq!(
            rfl_stream_send_bytes(s, buf1.as_ptr(), buf1.len()),
            rfl_status::Ok
        );
        assert_eq!(
            rfl_stream_send_bytes(s, buf2.as_ptr(), buf2.len()),
            rfl_status::Ok
        );
        assert_eq!(rfl_stream_end(s), rfl_status::Ok);

        // Convert to a message, then recover the receiver from the message.
        let msg = rfl_stream_into_message(s);
        assert!(!msg.is_null());
        assert_eq!(
            rfl_message_get_kind(msg),
            rfl_message_kind::StreamHandle
        );
        let rx = rfl_message_stream_take(msg);
        assert!(!rx.is_null());

        // Drain the frames.
        let mut collected: Vec<Vec<u8>> = Vec::new();
        let mut saw_end = false;
        for _ in 0..10 {
            let mut kind = rfl_stream_frame_kind::Timeout;
            let mut data: *const u8 = std::ptr::null();
            let mut len: usize = 0;
            let mut err: *mut c_char = std::ptr::null_mut();
            let st = rfl_stream_recv_next(rx, 250, &mut kind, &mut data, &mut len, &mut err);
            assert_eq!(st, rfl_status::Ok);
            match kind {
                rfl_stream_frame_kind::Data => {
                    assert!(!data.is_null());
                    collected.push(std::slice::from_raw_parts(data, len).to_vec());
                }
                rfl_stream_frame_kind::End | rfl_stream_frame_kind::Closed => {
                    saw_end = true;
                    break;
                }
                rfl_stream_frame_kind::Error => {
                    panic!(
                        "unexpected Error frame: {:?}",
                        if err.is_null() {
                            None
                        } else {
                            let s = CStr::from_ptr(err).to_str().ok().map(String::from);
                            rfl_string_free(err);
                            s
                        }
                    );
                }
                rfl_stream_frame_kind::Timeout => continue,
                rfl_stream_frame_kind::Begin => continue,
            }
        }
        assert!(saw_end, "never saw End frame");
        assert_eq!(collected, vec![buf1.to_vec(), buf2.to_vec()]);

        rfl_stream_recv_free(rx);
        rfl_message_free(msg);
    }
}

#[test]
fn subgraph_builder_with_registered_actors() {
    // Build an export that uses a component name not in the catalog.
    let export = serde_json::json!({
        "caseSensitive": false,
        "processes": {
            "tap": { "id": "tap", "component": "my_custom", "metadata": null }
        },
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {},
        "groups": []
    });

    unsafe extern "C" fn noop_callback(
        _ud: *mut std::ffi::c_void,
        _ctx: *mut rfl_actor_ctx,
    ) -> rfl_status {
        rfl_status::Ok
    }

    unsafe {
        let component = cstr("my_custom");
        let in_names = [cstr("in")];
        let in_ptrs: Vec<*const c_char> = in_names.iter().map(|s| s.as_ptr()).collect();
        let actor = rfl_actor_new(
            component.as_ptr(),
            in_ptrs.as_ptr(),
            1,
            std::ptr::null(),
            0,
            0,
            Some(noop_callback),
            std::ptr::null_mut(),
            None,
        );
        assert!(!actor.is_null());

        let builder = rfl_subgraph_builder_new(cstr(&export.to_string()).as_ptr());
        assert!(!builder.is_null());

        let reg_name = cstr("my_custom");
        assert_eq!(
            rfl_subgraph_builder_register_actor(builder, reg_name.as_ptr(), actor),
            rfl_status::Ok
        );

        let sg = rfl_subgraph_builder_build(builder);
        assert!(!sg.is_null(), "builder should produce a subgraph");

        let net = rfl_network_new();
        let tpl = cstr("tpl_sg");
        assert_eq!(
            rfl_network_register_actor(net, tpl.as_ptr(), sg),
            rfl_status::Ok
        );
        rfl_network_free(net);
        rfl_runtime_shutdown();
    }
}

#[test]
fn network_with_custom_config_json() {
    // Build the shape from the Rust default so we don't have to hand-
    // roll every nested field (TracingConfig etc. evolve over time).
    let default_cfg: reflow_rt::network::network::NetworkConfig = Default::default();
    let cfg_json = serde_json::to_string(&default_cfg).unwrap();

    unsafe {
        let cfg = cstr(&cfg_json);
        let net = rfl_network_new_with_config(cfg.as_ptr());
        if net.is_null() {
            let err = rfl_last_error_message();
            let msg = if !err.is_null() {
                let s = CStr::from_ptr(err).to_str().unwrap().to_owned();
                rfl_string_free(err);
                s
            } else {
                String::from("(no error)")
            };
            panic!("network_new_with_config failed: {msg}");
        }
        rfl_network_free(net);
        rfl_runtime_shutdown();
    }
}
