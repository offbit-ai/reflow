//! Template-catalog + subgraph bindings smoke test.
//!
//! Covers:
//!   1. `rfl_template_list_json` returns at least one known template.
//!   2. `rfl_template_actor_new` returns a usable handle for a known
//!      template and NULL for an unknown one.
//!   3. `rfl_subgraph_actor_new_from_json` builds a SubgraphActor that
//!      round-trips a graph export.

#![cfg(feature = "components")]

use reflow_rt_capi::*;
use std::ffi::{CStr, CString};

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn template_list_includes_known_ids() {
    unsafe {
        let ptr = rfl_template_list_json();
        assert!(!ptr.is_null());
        let json = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
        rfl_string_free(ptr);

        let ids: Vec<String> = serde_json::from_str(&json).unwrap();
        assert!(
            ids.iter().any(|id| id == "tpl_passthrough"),
            "expected tpl_passthrough in catalog, got {:?}",
            &ids[..ids.len().min(5)]
        );
    }
}

#[test]
fn template_actor_new_resolves_bundled_template() {
    unsafe {
        let ok = rfl_template_actor_new(cstr("tpl_passthrough").as_ptr());
        assert!(!ok.is_null(), "tpl_passthrough should resolve");
        rfl_actor_free(ok);

        let bad = rfl_template_actor_new(cstr("tpl_does_not_exist").as_ptr());
        assert!(bad.is_null(), "unknown template must return NULL");
    }
}

#[test]
fn subgraph_builds_and_registers() {
    // Minimal valid GraphExport JSON — uses a bundled template.
    let export = serde_json::json!({
        "caseSensitive": false,
        "processes": {
            "tap": { "id": "tap", "component": "tpl_passthrough", "metadata": null }
        },
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {},
        "groups": []
    });

    unsafe {
        let s = cstr(&export.to_string());
        let sg = rfl_subgraph_actor_new_from_json(s.as_ptr());
        assert!(!sg.is_null(), "subgraph should build from valid export");

        // Register it on a fresh network — exercises the shared actor handle.
        let net = rfl_network_new();
        assert!(!net.is_null());
        let tpl = cstr("tpl_embedded");
        assert_eq!(
            rfl_network_register_actor(net, tpl.as_ptr(), sg),
            rfl_status::Ok
        );
        rfl_network_free(net);

        rfl_runtime_shutdown();
    }
}
