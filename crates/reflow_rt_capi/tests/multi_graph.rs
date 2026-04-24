//! Multi-graph composition smoke test for the C ABI.

use reflow_rt_capi::*;
use std::ffi::{CStr, CString};

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn compose_two_graphs_produces_merged_export() {
    // Two tiny graphs, each with one node referencing a different
    // component; we compose them with no cross-graph connection.
    let composition = serde_json::json!({
        "graphs": [
            {
                "caseSensitive": false,
                "processes": { "a": { "id": "a", "component": "tpl_passthrough", "metadata": null } },
                "connections": [], "inports": {}, "outports": {},
                "properties": { "name": "left" }, "groups": []
            },
            {
                "caseSensitive": false,
                "processes": { "b": { "id": "b", "component": "tpl_passthrough", "metadata": null } },
                "connections": [], "inports": {}, "outports": {},
                "properties": { "name": "right" }, "groups": []
            }
        ],
        "connections": [],
        "shared_resources": [],
        "properties": { "name": "composed" },
        "case_sensitive": false
    });

    let input = cstr(&composition.to_string());
    let out = unsafe { rfl_compose_graphs(input.as_ptr()) };

    if out.is_null() {
        let err = rfl_last_error_message();
        let msg = if err.is_null() {
            "(no error message)".into()
        } else {
            let s = unsafe { CStr::from_ptr(err).to_str().unwrap().to_owned() };
            unsafe { rfl_string_free(err) };
            s
        };
        panic!("compose returned null: {msg}");
    }

    let raw = unsafe { CStr::from_ptr(out).to_str().unwrap().to_owned() };
    unsafe { rfl_string_free(out) };

    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let processes = v.get("processes").unwrap().as_object().unwrap();
    // The two nodes survive the composition under namespace-prefixed keys.
    assert!(
        processes
            .keys()
            .any(|k| k.contains("a") || k.contains("left/a")),
        "missing 'a' in composed processes: {:?}",
        processes.keys().collect::<Vec<_>>()
    );
    assert!(
        processes
            .keys()
            .any(|k| k.contains("b") || k.contains("right/b")),
        "missing 'b' in composed processes: {:?}",
        processes.keys().collect::<Vec<_>>()
    );
}

#[test]
fn compose_with_cross_graph_connection() {
    let composition = serde_json::json!({
        "graphs": [
            {
                "caseSensitive": false,
                "processes": { "src": { "id": "src", "component": "tpl_passthrough", "metadata": null } },
                "connections": [],
                "inports": {}, "outports": {},
                "properties": { "name": "gsrc" }, "groups": []
            },
            {
                "caseSensitive": false,
                "processes": { "sink": { "id": "sink", "component": "tpl_passthrough", "metadata": null } },
                "connections": [],
                "inports": {}, "outports": {},
                "properties": { "name": "gsink" }, "groups": []
            }
        ],
        "connections": [
            {
                "from": { "process": "gsrc/src", "port": "out" },
                "to":   { "process": "gsink/sink", "port": "in" }
            }
        ],
        "shared_resources": [],
        "properties": { "name": "pipeline" },
        "case_sensitive": false
    });

    let input = cstr(&composition.to_string());
    let out = unsafe { rfl_compose_graphs(input.as_ptr()) };
    assert!(!out.is_null(), "compose returned null");
    let raw = unsafe { CStr::from_ptr(out).to_str().unwrap().to_owned() };
    unsafe { rfl_string_free(out) };
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let conns = v.get("connections").unwrap().as_array().unwrap();
    assert!(
        !conns.is_empty(),
        "expected composed connection, got: {}",
        raw
    );
}

#[test]
fn compose_rejects_invalid_json() {
    let input = cstr("not json");
    let out = unsafe { rfl_compose_graphs(input.as_ptr()) };
    assert!(out.is_null());
    let err = rfl_last_error_message();
    assert!(!err.is_null());
    unsafe {
        let msg = CStr::from_ptr(err).to_str().unwrap().to_owned();
        rfl_string_free(err);
        assert!(msg.contains("parse"), "unexpected error: {msg}");
    }
}
