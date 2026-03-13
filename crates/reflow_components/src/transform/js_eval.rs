//! JavaScript expression evaluation helpers for data operations.
//!
//! These are internal utilities for evaluating inline JS expressions
//! within data operation configurations (filter, sort, transform, etc.).
//! This is NOT script execution — that goes through dynASB.

use anyhow::Result;
use reflow_actor::message::Message;
use rquickjs::{Context as JsContext, Runtime};
use serde_json::Value;
use std::collections::HashMap;

/// Create a JS runtime with `input` and `data` globals bound.
///
/// - `input.get('portName')` returns the serialized Message for that port
/// - `data` is bound to `context_data` (the current pipeline value)
fn with_js_context<F, T>(
    context_data: &Value,
    inputs: Option<&HashMap<String, Message>>,
    f: F,
) -> Result<T>
where
    F: FnOnce(&rquickjs::Ctx) -> Result<T>,
{
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;

    ctx.with(|ctx| {
        let globals = ctx.globals();

        // Bind `data` global
        let js_data: rquickjs::Value = ctx.json_parse(context_data.to_string())?;
        globals.set("data", js_data)?;

        // Bind `input` global with a real get() method backed by port data
        if let Some(inputs) = inputs {
            let ports_json = serialize_ports(inputs)?;
            let js_ports: rquickjs::Value = ctx.json_parse(ports_json)?;
            globals.set("__ports", js_ports)?;

            ctx.eval::<(), _>(
                r#"var input = { get: function(name) { return __ports[name] || null; } };"#,
            )?;
        }

        f(&ctx)
    })
}

/// Serialize all port Messages into a JSON object: { "portName": <serialized Message>, ... }
fn serialize_ports(inputs: &HashMap<String, Message>) -> Result<String> {
    let mut port_map = serde_json::Map::new();
    for (name, msg) in inputs {
        port_map.insert(name.clone(), serde_json::to_value(msg)?);
    }
    Ok(Value::Object(port_map).to_string())
}

/// Evaluate a JavaScript expression with both `data` and `input` globals.
pub(crate) fn evaluate_js_expression_with_inputs(
    expression: &str,
    context_data: &Value,
    inputs: Option<&HashMap<String, Message>>,
) -> Result<Value> {
    with_js_context(context_data, inputs, |ctx| {
        let wrapped_expr = format!("(function(data) {{ return {}; }})(data)", expression);
        let js_result: rquickjs::Value = ctx.eval(wrapped_expr.as_str())?;

        let json_str = if let Some(s) = ctx.json_stringify(js_result)? {
            s.to_string()?
        } else {
            "null".to_string()
        };

        let result: Value = serde_json::from_str(&json_str)?;
        Ok(result)
    })
}

/// Evaluate a JS filter with both `item` (as `data`) and `input` globals.
pub(crate) fn evaluate_js_filter_with_inputs(
    expression: &str,
    item: &Value,
    inputs: Option<&HashMap<String, Message>>,
) -> Result<bool> {
    with_js_context(item, inputs, |ctx| {
        // Also alias `data` as `item` for filter expressions
        ctx.eval::<(), _>("var item = data;")?;

        let filter_fn = format!("(function(item) {{ return {}; }})(item)", expression);
        let result: bool = ctx.eval(filter_fn)?;
        Ok(result)
    })
}

/// Resolve a template string containing `${...}` JS expressions.
///
/// Evaluates as a JS template literal with `input` and `data` in scope.
/// E.g. `${input.get('prices').data.total}` evaluates via real JS.
pub(crate) fn resolve_template_string(
    template: &str,
    context_data: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<String> {
    // If no template expressions, return as-is
    if !template.contains("${") {
        return Ok(template.to_string());
    }

    with_js_context(context_data, Some(inputs), |ctx| {
        // Evaluate as a JS template literal using backticks
        let js_expr = format!("`{}`", template);
        let result: String = ctx.eval(js_expr)?;
        Ok(result)
    })
}
