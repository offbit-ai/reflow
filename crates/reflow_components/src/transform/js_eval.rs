//! JavaScript expression evaluation helpers for data operations.
//!
//! These are internal utilities for evaluating inline JS expressions
//! within data operation configurations (filter, sort, transform, etc.).
//! This is NOT script execution — that goes through dynASB.

use anyhow::Result;
use rquickjs::{Context as JsContext, Runtime};
use serde_json::Value;

/// Evaluate a JavaScript expression with the given data as context.
pub(crate) fn evaluate_js_expression(expression: &str, context_data: &Value) -> Result<Value> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;

    ctx.with(|ctx| {
        let globals = ctx.globals();
        let js_data: rquickjs::Value = ctx.json_parse(context_data.to_string())?;
        globals.set("data", js_data)?;

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

/// Evaluate a JavaScript filter expression against an item, returning a boolean.
pub(crate) fn evaluate_js_filter(expression: &str, item: &Value) -> Result<bool> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;

    ctx.with(|ctx| {
        let globals = ctx.globals();
        let js_item: rquickjs::Value = ctx.json_parse(item.to_string())?;
        globals.set("item", js_item)?;

        let filter_fn = format!("(function(item) {{ return {}; }})(item)", expression);
        let result: bool = ctx.eval(filter_fn)?;

        Ok(result)
    })
}
