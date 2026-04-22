//! Shared helpers for crate-local display components.

use reflow_network::template::DisplayComponent;

const UI_LIB_JS: &str = include_str!("display/reflow-ui.js");

pub(crate) fn inline_source(source: &str) -> String {
    format!("if(!globalThis.ReflowUI){{{}}}\n{}", UI_LIB_JS, source)
}

pub(crate) fn inline_display(
    element: &str,
    source: &str,
    observed: &[&str],
    width: Option<&str>,
) -> DisplayComponent {
    DisplayComponent {
        element: element.to_string(),
        bundle_id: None,
        source: Some(inline_source(source)),
        shadow: Some(true),
        observed_props: Some(observed.iter().map(|s| s.to_string()).collect()),
        width: width.map(|w| w.to_string()),
    }
}
