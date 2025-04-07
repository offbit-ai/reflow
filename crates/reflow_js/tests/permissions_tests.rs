use reflow_js::JavascriptRuntime;
use deno_runtime::deno_core::{self, error::AnyError, v8};

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
mod permissions_tests {
    use super::*;



    #[tokio::test]
    async fn test_filesystem_operations() -> Result<(), AnyError> {
        // Test filesystem operations with permissions
        let mut runtime = JavascriptRuntime::new()?;
        
        // Create a temporary file and read it back
        let result = runtime.execute(
            "fs_test", 
            r#"const tempFilename = 'temp_test_file.txt';
            const encoder = new TextEncoder();
            const data = encoder.encode('Hello, filesystem!');
            
            try {
                // Write to file
                Deno.writeFileSync(tempFilename, data);
                
                // Read from file
                const readData = Deno.readFileSync(tempFilename);
                const decoder = new TextDecoder();
                const text = decoder.decode(readData);
                
                // Clean up
                Deno.removeSync(tempFilename);
                
                text === 'Hello, filesystem!';
            } catch (e) {
                console.error('Filesystem error:', e);
                false;
            }
            "#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Filesystem operations test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_network_permissions() -> Result<(), AnyError> {
        // Test network operations with permissions
        let mut runtime = JavascriptRuntime::new()?;
        
        // Attempt to make a network request
        let result = runtime.execute(
            "network_test", 
            r#"async function testNetwork() {
                try {
                    const response = await fetch('https://jsonplaceholder.typicode.com/todos/1');
                    return response.status === 200;
                } catch (e) {
                    console.error('Network error:', e);
                    return false;
                }
            }
            testNetwork()"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Network permissions test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_websocket_permissions() -> Result<(), AnyError> {
        // Test WebSocket operations with permissions
        let mut runtime = JavascriptRuntime::new()?;
        
        // Test WebSocket API availability (not actually connecting)
        let result = runtime.execute(
            "websocket_test", 
            r#"try {
                // Just test if the WebSocket constructor is available
                typeof WebSocket === 'function';
            } catch (e) {
                console.error('WebSocket error:', e);
                false;
            }"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "WebSocket permissions test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_web_apis() -> Result<(), AnyError> {
        // Test various Web APIs that should be available
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "web_apis_test", 
            r#"const apis = [
                typeof URL === 'function',
                typeof Blob === 'function',
                typeof TextEncoder === 'function',
                typeof TextDecoder === 'function',
                typeof console === 'object',
                typeof fetch === 'function'
            ];
            
            apis.every(available => available === true)"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Web APIs test failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_web_storage() -> Result<(), AnyError> {
        // Test localStorage and sessionStorage APIs
        let mut runtime = JavascriptRuntime::new()?;
        
        let result = runtime.execute(
            "web_storage_test", 
            r#"try {
                // Test localStorage
                localStorage.setItem('test', 'value');
                const value = localStorage.getItem('test');
                localStorage.removeItem('test');
                
                value === 'value';
            } catch (e) {
                console.error('Web Storage error:', e);
                false;
            }"#
        ).await?;
        
        // Verify the result
        let result_str = extract_string_result(&mut runtime, result)?;
        assert_eq!(result_str, "true", "Web Storage test failed");
        Ok(())
    }
}
