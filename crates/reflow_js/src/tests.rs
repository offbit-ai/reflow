use crate::JavascriptRuntime;
use deno_runtime::deno_core::{self, error::AnyError};
use std::sync::Arc;
use tokio;

#[cfg(test)]
mod tests {
    use super::*;
    use deno_runtime::deno_core::v8;

    #[tokio::test]
    async fn test_runtime_initialization() {
        // Test that the runtime can be created successfully
        let runtime = JavascriptRuntime::new();
        assert!(runtime.is_ok(), "Failed to initialize JavascriptRuntime");
    }

    #[tokio::test]
    async fn test_basic_script_execution() -> Result<(), AnyError> {
        // Test executing a simple script
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute("test", "2 + 2").await?;

        // Convert the result to a string for comparison
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "4", "Basic arithmetic failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_console_log() -> Result<(), AnyError> {
        // Test that console.log works
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime
            .execute("console_test", "console.log('Hello, world!'); true")
            .await?;

        // Verify the result is true
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "true", "Console log test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_promise_resolution() -> Result<(), AnyError> {
        // Test that promises are properly resolved
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime
            .execute(
                "promise_test",
                "new Promise(resolve => resolve('promise resolved'))",
            )
            .await?;

        // Verify the promise was resolved with the expected value
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "promise resolved", "Promise resolution failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_promise_rejection() {
        // Test that promise rejections are properly handled
        let mut runtime = JavascriptRuntime::new().unwrap();
        let result = runtime
            .execute(
                "promise_rejection_test",
                "new Promise((_, reject) => reject(new Error('test error')))",
            )
            .await;

        assert!(result.is_err(), "Promise rejection was not detected");
        if let Err(e) = result {
            assert!(
                e.to_string().contains("test error"),
                "Error message doesn't match: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_syntax_error() {
        // Test that syntax errors are properly reported
        let mut runtime = JavascriptRuntime::new().unwrap();
        let result = runtime.execute("syntax_error_test", "2 +* 2").await;

        assert!(result.is_err(), "Syntax error was not detected");
    }

    #[tokio::test]
    async fn test_async_execution() -> Result<(), AnyError> {
        // Test async/await functionality
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime
            .execute(
                "async_test",
                r#"
            async function test() { 
                await new Promise(resolve => setTimeout(resolve, 100)); 
                return "async completed"; 
            } 
            test()"#,
            )
            .await?;

        // Verify the async function completed successfully
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "async completed", "Async execution failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_api() -> Result<(), AnyError> {
        // Test that fetch API works
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime
            .execute(
                "fetch_test",
                r#"async function testFetch() {
                try {
                    const response = await fetch('https://jsonplaceholder.typicode.com/todos/1');
                    const data = await response.json();
                    return data.title ? true : false;
                } catch (e) {
                    return false;
                }
            }
            testFetch()"#,
            )
            .await?;

        // Verify the fetch was successful
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "true", "Fetch API test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_executions() -> Result<(), AnyError> {
        // Test multiple script executions in the same runtime
        let mut runtime = JavascriptRuntime::new()?;

        // First execution
        let _ = runtime.execute("first", "var x = 10;").await?;

        // Second execution that uses the variable from the first
        let result = runtime.execute("second", "x + 5").await?;

        // Verify the result
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(result_rust, "15", "Multiple executions test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_register_function() -> Result<(), AnyError> {
        // Create a new runtime
        let mut runtime = JavascriptRuntime::new()?;

        // Define a Rust function to be called from JavaScript
        let rust_add = move |scope: &mut v8::HandleScope,
                        args: v8::FunctionCallbackArguments,
                        mut rv: v8::ReturnValue| {
            // Extract arguments
            if args.length() < 2 {
                return;
            }

            // Convert JS numbers to Rust
            let a = args.get(0).number_value(scope).unwrap_or(0.0);
            let b = args.get(1).number_value(scope).unwrap_or(0.0);

            // Perform the addition
            let result = a + b;

            // Return the result to JavaScript
            rv.set(v8::Number::new(scope, result).into());
        };

        // Register the Rust function in the JavaScript runtime
        runtime.register_function("rustAdd", rust_add);

        // Execute JavaScript code that calls the Rust function
        let result = runtime
            .execute(
                "test_rust_function",
                "const result = rustAdd(5, 7); result === 12",
            )
            .await?;

        // Verify the result
        let mut runner = runtime.worker.write();
        let scope = &mut runner.js_runtime.handle_scope();
        let local = v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);

        assert_eq!(
            result_rust, "true",
            "Rust function registration test failed"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_call_function() -> Result<(), AnyError> {
        // Create a new runtime
        let mut runtime = JavascriptRuntime::new()?;

        // First, define a JavaScript function
        runtime
            .execute(
                "define_function",
                "function multiply(a, b) { return a * b; }",
            )
            .await?;

        // Create arguments for the function call
        let arg1 = runtime.create_number(6.0);
        let arg2 = runtime.create_number(7.0);
      
        let args = vec![arg1, arg2];

        // Call the JavaScript function from Rust
        let result = runtime.call_function("multiply", &args)?;

        // Verify the result
        let mut runner = runtime.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let local = v8::Local::new(&mut scope, result);
        let result_str = local.to_string(&mut scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(&mut scope);

        assert_eq!(result_rust, "42", "Function call test failed");
        Ok(())
    }
}
