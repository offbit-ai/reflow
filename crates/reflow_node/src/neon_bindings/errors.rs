//! Error type bindings for Neon
//! 
//! Provides clean error types that mirror the WASM version but without prefixes

use neon::prelude::*;

/// CompositionError - Clean name, no WASM* prefix
pub struct CompositionError {
    message: String,
}

impl Finalize for CompositionError {}

impl CompositionError {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<CompositionError>> {
        let message = cx.argument::<JsString>(0)?.value(&mut cx);
        Ok(cx.boxed(CompositionError { message }))
    }

    fn get_message(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<CompositionError>>()?;
        Ok(cx.string(&this.message))
    }
}

/// ValidationError - Clean name, no WASM* prefix
pub struct ValidationError {
    message: String,
}

impl Finalize for ValidationError {}

impl ValidationError {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<ValidationError>> {
        let message = cx.argument::<JsString>(0)?.value(&mut cx);
        Ok(cx.boxed(ValidationError { message }))
    }

    fn get_message(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<ValidationError>>()?;
        Ok(cx.string(&this.message))
    }
}

/// LoadError - Clean name, no WASM* prefix
pub struct LoadError {
    message: String,
}

impl Finalize for LoadError {}

impl LoadError {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<LoadError>> {
        let message = cx.argument::<JsString>(0)?.value(&mut cx);
        Ok(cx.boxed(LoadError { message }))
    }

    fn get_message(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<LoadError>>()?;
        Ok(cx.string(&this.message))
    }
}

/// NamespaceError - Clean name, no WASM* prefix
pub struct NamespaceError {
    message: String,
}

impl Finalize for NamespaceError {}

impl NamespaceError {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NamespaceError>> {
        let message = cx.argument::<JsString>(0)?.value(&mut cx);
        Ok(cx.boxed(NamespaceError { message }))
    }

    fn get_message(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<NamespaceError>>()?;
        Ok(cx.string(&this.message))
    }
}

/// Export functions for creating error instances
pub fn create_composition_error(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, CompositionError::new)?)
}

pub fn create_validation_error(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, ValidationError::new)?)
}

pub fn create_load_error(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, LoadError::new)?)
}

pub fn create_namespace_error(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NamespaceError::new)?)
}
