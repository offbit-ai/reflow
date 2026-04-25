//! Reflow actor pack: 6,700+ generated API-service actors.
//!
//! Unlike the smaller packs, this one doesn't hard-code a template-id
//! list — it iterates [`reflow_components::get_api_template_infos`] at
//! pack-load time and registers every entry. That keeps the pack binary
//! immune to additions/removals in `reflow_api_services`'s generator.
//!
//! `reflow_components` deliberately hides the api-services re-exports
//! when `cfg(clippy)` is set (the generated catalog stalls clippy on
//! ~6,700 actors). We mirror that gate: under clippy the pack
//! registers nothing; the real build (no `clippy` cfg) sees the full
//! registry.

use reflow_pack_sdk::{PackHost, reflow_pack};

#[reflow_pack]
fn register(host: &mut PackHost) {
    register_all(host);
}

#[cfg(not(clippy))]
fn register_all(host: &mut PackHost) {
    let infos = reflow_components::get_api_template_infos();
    if infos.is_empty() {
        return;
    }
    for info in infos {
        let tid = info.template_id.to_string();
        host.register(info.template_id, move || {
            reflow_components::get_api_actor_for_template(&tid)
                .unwrap_or_else(|| panic!("api-services pack: template '{tid}' missing from registry"))
        });
    }
}

#[cfg(clippy)]
fn register_all(_host: &mut PackHost) {}
