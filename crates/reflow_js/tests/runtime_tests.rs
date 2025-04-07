use reflow_js::JavascriptRuntime;
use deno_runtime::deno_core::{self, error::AnyError};
use std::sync::Arc;

#[cfg(test)]
mod runtime_tests {
    use super::*;

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
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
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
        let result = runtime.execute("console_test", "console.log('Hello, world!'); true").await?;
        
        // Verify the result is true
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
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
        let result = runtime.execute(
            "promise_test", 
            "new Promise(resolve => resolve('promise resolved'))"
        ).await?;
        
        // Verify the promise was resolved with the expected value
        let runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
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
        let result = runtime.execute(
            "promise_rejection_test", 
            "new Promise((_, reject) => reject(new Error('test error')))"
        ).await;
        
        assert!(result.is_err(), "Promise rejection was not detected");
        if let Err(e) = result {
            assert!(e.to_string().contains("test error"), "Error message doesn't match: {}", e);
        }
    }

    #[tokio::test]
    async fn test_syntax_error() {
        // Test that syntax errors are properly reported
        let mut runtime = JavascriptRuntime::new().unwrap();
        let result = runtime.execute("syntax_error_test", "2 +* 2").await;
        
        assert!(result.is_err(), "Syntax error was not detected");
        if let Err(e) = result {
            assert!(e.to_string().contains("SyntaxError"), "Error should be a syntax error: {}", e);
        }
    }

    #[tokio::test]
    async fn test_reference_error() {
        // Test that reference errors are properly reported
        let mut runtime = JavascriptRuntime::new().unwrap();
        let result = runtime.execute("reference_error_test", "nonExistentVariable + 2").await;
        
        assert!(result.is_err(), "Reference error was not detected");
        if let Err(e) = result {
            assert!(e.to_string().contains("ReferenceError"), "Error should be a reference error: {}", e);
        }
    }

    #[tokio::test]
    async fn test_async_execution() -> Result<(), AnyError> {
        // Test async/await functionality
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute(
            "async_test", 
            r#"async function test() { 
                await new Promise(resolve => setTimeout(resolve, 100)); 
                return "async completed"; 
            } 
            test()"#
        ).await?;
        
        // Verify the async function completed successfully
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
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
        let result = runtime.execute(
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
            testFetch()"#
        ).await?;
        
        // Verify the fetch was successful
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
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
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);
        
        assert_eq!(result_rust, "15", "Multiple executions test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_complex_object() -> Result<(), AnyError> {
        // Test handling of complex JavaScript objects
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute(
            "complex_object_test", 
            r#"const obj = { 
                name: 'test', 
                value: 42, 
                nested: { a: 1, b: 2 },
                method: function() { return this.value; }
            };
            obj.method()"#
        ).await?;
        
        // Verify the result
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);
        
        assert_eq!(result_rust, "42", "Complex object test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_json_parsing() -> Result<(), AnyError> {
        // Test JSON parsing capabilities
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute(
            "json_test", 
            r#"const jsonStr = '{"name":"test","values":[1,2,3]}';
            const obj = JSON.parse(jsonStr);
            obj.values.reduce((sum, val) => sum + val, 0)"#
        ).await?;
        
        // Verify the result
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);
        
        assert_eq!(result_rust, "6", "JSON parsing test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_error_stack_trace() {
        // Test that error stack traces are properly captured
        let mut runtime = JavascriptRuntime::new().unwrap();
        let result = runtime.execute(
            "stack_trace_test", 
            r#"function foo() { return bar(); }
            function bar() { return baz(); }
            function baz() { throw new Error('test error'); }
            foo();"#
        ).await;
        
        assert!(result.is_err(), "Error was not thrown");
        if let Err(e) = result {
            let error_string = e.to_string();
            assert!(error_string.contains("test error"), "Error message not found");
            assert!(error_string.contains("baz"), "Function 'baz' not in stack trace");
            assert!(error_string.contains("bar"), "Function 'bar' not in stack trace");
            assert!(error_string.contains("foo"), "Function 'foo' not in stack trace");
        }
    }

    #[tokio::test]
    async fn test_timeout_execution() -> Result<(), AnyError> {
        // Test setTimeout functionality
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute(
            "timeout_test", 
            r#"let result = 'not executed';
            async function test(){
                await new Promise(resolve => {
                    setTimeout(() => {
                        result = 'executed';
                        resolve();
                    }, 100);
                });
                return result;
            }
            test()"#
        ).await?;
        
        // Verify the timeout was executed
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);
        
        assert_eq!(result_rust, "executed", "Timeout execution failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_promises() -> Result<(), AnyError> {
        // Test handling of multiple concurrent promises
        let mut runtime = JavascriptRuntime::new()?;
        let result = runtime.execute(
            "concurrent_promises_test", 
            r#"async function test() {
                const promises = [
                    new Promise(resolve => setTimeout(() => resolve(1), 100)),
                    new Promise(resolve => setTimeout(() => resolve(2), 50)),
                    new Promise(resolve => setTimeout(() => resolve(3), 150))
                ];
                
                const results = await Promise.all(promises);
                return results.reduce((sum, val) => sum + val, 0);
            }
            test()"#
        ).await?;
        
        // Verify all promises were resolved correctly
        let mut runner = &mut runtime.worker.write().js_runtime;
        let scope = &mut runner.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let result_str = local.to_string(scope).unwrap();
        let result_rust = result_str.to_rust_string_lossy(scope);
        
        assert_eq!(result_rust, "6", "Concurrent promises test failed");
        Ok(())
    }
}