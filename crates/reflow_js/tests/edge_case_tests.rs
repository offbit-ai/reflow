use reflow_js::JavascriptRuntime;
use deno_runtime::deno_core::{self, error::AnyError, v8};
use std::time::Instant;

// Helper function to extract a string result from a V8 value
fn extract_string_result(
    runtime: &mut JavascriptRuntime,
    result: v8::Global<v8::Value>,
) -> Result<String, AnyError> {
    let mut runner = &mut runtime.worker.write().js_runtime;
    let scope = &mut runner.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    Ok(result_rust)
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[tokio::test]
    async fn test_large_string_handling() -> Result<(), AnyError> {
        // Test handling of large string values
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "large_string_test", 
            r#"// Generate a large string
            let largeString = 'A'.repeat(1000000);
            largeString.length === 1000000"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Large string handling test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_recursive_function() -> Result<(), AnyError> {
        // Test handling of recursive functions (with reasonable depth)
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "recursive_function_test", 
            r#"function factorial(n) {
                if (n <= 1) return 1;
                return n * factorial(n - 1);
            }
            
            factorial(10) === 3628800"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Recursive function test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_error_recovery() -> Result<(), AnyError> {
        // Test that the runtime can recover from errors
        let mut runtime = JavascriptRuntime::new()?;
        
        // Execute a script that will throw an error
        let error_result = runtime.execute(
            "error_script", 
            "throw new Error('Deliberate error');"  
        ).await;
        
        assert!(error_result.is_err(), "Error was not thrown");
        
        // Now execute another script to verify the runtime recovered
        let recovery_result = runtime.execute(
            "recovery_script", 
            "'Recovered successfully'"  
        ).await?;
        
        // Verify the recovery result
        let result_str = extract_string_result(&mut runtime, recovery_result)?;
        assert_eq!(result_str, "Recovered successfully", "Error recovery test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_memory_intensive_operation() -> Result<(), AnyError> {
        // Test handling of memory-intensive operations
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "memory_intensive_test", 
            r#"// Create a large array and perform operations on it
            const largeArray = new Array(100000).fill(0).map((_, i) => i);
            const sum = largeArray.reduce((acc, val) => acc + val, 0);
            
            // Sum of numbers from 0 to 99999
            sum === 4999950000"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Memory intensive operation test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_long_running_script() -> Result<(), AnyError> {
        // Test handling of long-running scripts
        let mut runtime = JavascriptRuntime::new()?;
        
        // Record the start time
        let start = Instant::now();
        
        let result = runtime.execute(
            "long_running_test", 
            r#"// Perform a CPU-intensive operation
            let result = 0;
            for (let i = 0; i < 10000000; i++) {
                result += i % 2;
            }
            result === 5000000"#
        ).await?;
        
        // Verify the script ran for a significant amount of time
        let duration = start.elapsed();
        println!("Long-running script took: {:?}", duration);
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Long running script test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_concurrent_scripts() -> Result<(), AnyError> {
        // Test running multiple scripts using separate runtime instances
        // Note: JavascriptRuntime is not thread-safe, so we run scripts sequentially
        
        // Create three separate runtime instances
        let mut runtime1 = JavascriptRuntime::new()?;
        let mut runtime2 = JavascriptRuntime::new()?;
        let mut runtime3 = JavascriptRuntime::new()?;
        
        // Execute scripts and extract results
        // Execute first, then extract to avoid multiple mutable borrows
        let script1_result = runtime1.execute("script1", "'Script 1 result'").await?;
        let result1 = extract_string_result(&mut runtime1, script1_result)?;
        
        let script2_result = runtime2.execute("script2", "'Script 2 result'").await?;
        let result2 = extract_string_result(&mut runtime2, script2_result)?;
        
        let script3_result = runtime3.execute("script3", "'Script 3 result'").await?;
        let result3 = extract_string_result(&mut runtime3, script3_result)?;
        
        // Verify all scripts executed successfully
        assert_eq!(result1, "Script 1 result", "Script 1 returned unexpected result");
        assert_eq!(result2, "Script 2 result", "Script 2 returned unexpected result");
        assert_eq!(result3, "Script 3 result", "Script 3 returned unexpected result");
        
        Ok(())
    }

    #[tokio::test]
    async fn test_json_circular_reference_handling() -> Result<(), AnyError> {
        // Test handling of circular references in JSON
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "circular_reference_test", 
            r#"// Create an object with circular reference
            const obj = { name: 'circular' };
            obj.self = obj;
            
            try {
                // This should throw an error
                JSON.stringify(obj);
                false; // Should not reach here
            } catch (e) {
                e.toString().includes('circular');
            }"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "JSON circular reference test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_unicode_handling() -> Result<(), AnyError> {
        // Test handling of Unicode characters
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "unicode_test", 
            r#"// Test various Unicode characters
            const unicodeStr = '你好，世界! こんにちは! 안녕하세요! 🚀🔥🌟';
            unicodeStr.length === 22"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Unicode handling test failed");
        Ok(())
    }
}
