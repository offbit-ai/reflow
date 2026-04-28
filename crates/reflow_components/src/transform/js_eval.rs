//! JavaScript expression evaluation helpers for data operations.
//!
//! These are internal utilities for evaluating inline JS expressions
//! within data operation configurations (filter, sort, transform, etc.).
//! This is NOT script execution — that goes through dynASB.
//!
//! Two backends, picked at compile time:
//! - native: rquickjs runtime (embedded QuickJS)
//! - wasm32: js_sys::Function calling into the host browser engine

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use anyhow::Result;
    use reflow_actor::message::Message;
    use rquickjs::{Context as JsContext, Runtime};
    use serde_json::Value;
    use std::collections::HashMap;

    /// Create a JS runtime with `input` and `data` globals bound.
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

            let js_data: rquickjs::Value = ctx.json_parse(context_data.to_string())?;
            globals.set("data", js_data)?;

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

    fn serialize_ports(inputs: &HashMap<String, Message>) -> Result<String> {
        let mut port_map = serde_json::Map::new();
        for (name, msg) in inputs {
            port_map.insert(name.clone(), serde_json::to_value(msg)?);
        }
        Ok(Value::Object(port_map).to_string())
    }

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

    pub(crate) fn evaluate_js_filter_with_inputs(
        expression: &str,
        item: &Value,
        inputs: Option<&HashMap<String, Message>>,
    ) -> Result<bool> {
        with_js_context(item, inputs, |ctx| {
            ctx.eval::<(), _>("var item = data;")?;
            let filter_fn = format!("(function(item) {{ return {}; }})(item)", expression);
            let result: bool = ctx.eval(filter_fn)?;
            Ok(result)
        })
    }

    pub(crate) fn resolve_template_string(
        template: &str,
        context_data: &Value,
        inputs: &HashMap<String, Message>,
    ) -> Result<String> {
        if !template.contains("${") {
            return Ok(template.to_string());
        }

        with_js_context(context_data, Some(inputs), |ctx| {
            let js_expr = format!("`{}`", template);
            let result: String = ctx.eval(js_expr)?;
            Ok(result)
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    //! js_sys-backed implementation. Each call compiles a small Function
    //! object that takes JSON-encoded `data` and `ports` and returns a
    //! JSON-encoded result. We round-trip through JSON to keep the
    //! Rust↔JS boundary string-only — same shape as the native side.

    use anyhow::{anyhow, Result};
    use js_sys::Function;
    use reflow_actor::message::Message;
    use serde_json::Value;
    use std::collections::HashMap;
    use wasm_bindgen::JsValue;

    fn serialize_ports(inputs: &HashMap<String, Message>) -> Result<String> {
        let mut port_map = serde_json::Map::new();
        for (name, msg) in inputs {
            port_map.insert(name.clone(), serde_json::to_value(msg)?);
        }
        Ok(Value::Object(port_map).to_string())
    }

    /// Build the standard prelude that wires `data`, `__ports`, and the
    /// `input.get(name)` helper from the two string args.
    const PRELUDE: &str = r#"
        var data = JSON.parse(data_str);
        var __ports = JSON.parse(ports_str);
        var input = { get: function(name) { return __ports[name] || null; } };
    "#;

    fn call_eval(body: &str, data_str: &str, ports_str: &str) -> Result<JsValue> {
        let func = Function::new_with_args("data_str,ports_str", body);
        func.call2(
            &JsValue::NULL,
            &JsValue::from_str(data_str),
            &JsValue::from_str(ports_str),
        )
        .map_err(|e| anyhow!("JS eval failed: {:?}", js_sys::Object::from(e).to_string()))
    }

    pub(crate) fn evaluate_js_expression_with_inputs(
        expression: &str,
        context_data: &Value,
        inputs: Option<&HashMap<String, Message>>,
    ) -> Result<Value> {
        let data_str = context_data.to_string();
        let ports_str = match inputs {
            Some(inputs) => serialize_ports(inputs)?,
            None => "{}".to_string(),
        };

        let body = format!(
            "{prelude} return JSON.stringify((function(data) {{ return {expr}; }})(data));",
            prelude = PRELUDE,
            expr = expression,
        );

        let result = call_eval(&body, &data_str, &ports_str)?;
        let json_str = result.as_string().unwrap_or_else(|| "null".to_string());
        let parsed: Value = serde_json::from_str(&json_str)?;
        Ok(parsed)
    }

    pub(crate) fn evaluate_js_filter_with_inputs(
        expression: &str,
        item: &Value,
        inputs: Option<&HashMap<String, Message>>,
    ) -> Result<bool> {
        let data_str = item.to_string();
        let ports_str = match inputs {
            Some(inputs) => serialize_ports(inputs)?,
            None => "{}".to_string(),
        };

        let body = format!(
            "{prelude} var item = data; return Boolean((function(item) {{ return {expr}; }})(item));",
            prelude = PRELUDE,
            expr = expression,
        );

        let result = call_eval(&body, &data_str, &ports_str)?;
        Ok(result.is_truthy())
    }

    pub(crate) fn resolve_template_string(
        template: &str,
        context_data: &Value,
        inputs: &HashMap<String, Message>,
    ) -> Result<String> {
        if !template.contains("${") {
            return Ok(template.to_string());
        }

        let data_str = context_data.to_string();
        let ports_str = serialize_ports(inputs)?;

        let body = format!(
            "{prelude} return `{tpl}`;",
            prelude = PRELUDE,
            tpl = template,
        );

        let result = call_eval(&body, &data_str, &ports_str)?;
        result
            .as_string()
            .ok_or_else(|| anyhow!("template did not evaluate to a string"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{
    evaluate_js_expression_with_inputs, evaluate_js_filter_with_inputs, resolve_template_string,
};

#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::{
    evaluate_js_expression_with_inputs, evaluate_js_filter_with_inputs, resolve_template_string,
};
