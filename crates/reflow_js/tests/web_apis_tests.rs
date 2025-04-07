use reflow_js::JavascriptRuntime;
use deno_runtime::deno_core::{self, error::AnyError};

#[tokio::test]
async fn test_abort_controller() -> Result<(), AnyError> {
    let mut runtime = JavascriptRuntime::new()?;
    
    let script = r#"
    const controller = new AbortController();
    const signal = controller.signal;
    
    // Initially not aborted
    if (signal.aborted) {
        throw new Error("Signal should not be aborted initially");
    }
    
    // Abort the controller
    controller.abort();
    
    // Now it should be aborted
    if (!signal.aborted) {
        throw new Error("Signal should be aborted after calling abort()");
    }
    
    "All tests passed!"
    "#;
    
    let result = runtime.execute("abort_controller_test", script).await?;
    
    // Convert the result to a string
    let runtime_lock = &mut runtime.worker.write().js_runtime;
    let scope = &mut runtime_lock.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    
    assert_eq!(result_rust, "All tests passed!");
    
    Ok(())
}

#[tokio::test]
async fn test_blob() -> Result<(), AnyError> {
    let mut runtime = JavascriptRuntime::new()?;
    
    let script = r#"
    const blob = new Blob(['Hello, world!'], { type: 'text/plain' });
    
    // Check blob size
    if (blob.size !== 13) {
        throw new Error(`Expected blob size to be 13, got ${blob.size}`);
    }
    
    // Check blob type
    if (blob.type !== 'text/plain') {
        throw new Error(`Expected blob type to be 'text/plain', got ${blob.type}`);
    }
    
    "All tests passed!"
    "#;
    
    let result = runtime.execute("blob_test", script).await?;
    
    // Convert the result to a string
    let runtime_lock = &mut runtime.worker.write().js_runtime;
    let scope = &mut runtime_lock.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    
    assert_eq!(result_rust, "All tests passed!");
    
    Ok(())
}

#[tokio::test]
async fn test_text_encoder_decoder() -> Result<(), AnyError> {
    let mut runtime = JavascriptRuntime::new()?;
    
    let script = r#"
    const encoder = new TextEncoder();
    const encoded = encoder.encode('Hello, world!');
    
    // Check encoded length
    if (encoded.length !== 13) {
        throw new Error(`Expected encoded length to be 13, got ${encoded.length}`);
    }
    
    const decoder = new TextDecoder();
    const decoded = decoder.decode(encoded);
    
    // Check decoded text
    if (decoded !== 'Hello, world!') {
        throw new Error(`Expected decoded text to be 'Hello, world!', got ${decoded}`);
    }
    
    "All tests passed!"
    "#;
    
    let result = runtime.execute("text_encoder_decoder_test", script).await?;
    
    // Convert the result to a string
    let runtime_lock = &mut runtime.worker.write().js_runtime;
    let scope = &mut runtime_lock.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    
    assert_eq!(result_rust, "All tests passed!");
    
    Ok(())
}

#[tokio::test]
async fn test_performance_api() -> Result<(), AnyError> {
    let mut runtime = JavascriptRuntime::new()?;
    
    let script = r#"
    // Create a mark
    performance.mark('start');
    
    // Do some work
    for (let i = 0; i < 1000; i++) {
        // Just a simple loop
    }
    
    // Create another mark
    performance.mark('end');
    
    // Measure between the marks
    performance.measure('test', 'start', 'end');
    
    // Get the measure
    const entries = performance.getEntriesByName('test');
    
    // Check that we got a measure
    if (entries.length !== 1) {
        throw new Error(`Expected 1 performance entry, got ${entries.length}`);
    }
    
    // Check that the measure has a duration
    if (typeof entries[0].duration !== 'number' || entries[0].duration <= 0) {
        throw new Error(`Expected positive duration, got ${entries[0].duration}`);
    }
    
    "All tests passed!"
    "#;
    
    let result = runtime.execute("performance_api_test", script).await?;
    
    // Convert the result to a string
    let runtime_lock = &mut runtime.worker.write().js_runtime;
    let scope = &mut runtime_lock.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    
    assert_eq!(result_rust, "All tests passed!");
    
    Ok(())
}

#[tokio::test]
async fn test_streams_api() -> Result<(), AnyError> {
    let mut runtime = JavascriptRuntime::new()?;
    
    let script = r#"
    // Create a ReadableStream
    const readableStream = new ReadableStream({
        start(controller) {
            controller.enqueue('Hello');
            controller.enqueue('World');
            controller.close();
        }
    });
    
    // Create a TransformStream that converts to uppercase
    const transformStream = new TransformStream({
        transform(chunk, controller) {
            controller.enqueue(chunk.toUpperCase());
        }
    });
    
    // Create a WritableStream that collects the chunks
    let result = '';
    const writableStream = new WritableStream({
        write(chunk) {
            result += chunk;
        }
    });
    
    // Pipe the streams together
    const pipePromise = readableStream
        .pipeThrough(transformStream)
        .pipeTo(writableStream);
    
    
    const runPromise = async () => {
        // Wait for the pipeline to complete
        await pipePromise;
        // Check the result
        if (result !== 'HELLOWORLD') {
            throw new Error(`Expected result to be 'HELLOWORLD', got '${result}'`);
        }
        
        return "All tests passed!"
    };
    
    runPromise();
    "#;
    
    let result = runtime.execute("streams_api_test", script).await?;
    
    // Convert the result to a string
    let runtime_lock = &mut runtime.worker.write().js_runtime;
    let scope = &mut runtime_lock.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let result_str = local.to_string(scope).unwrap();
    let result_rust = result_str.to_rust_string_lossy(scope);
    
    assert_eq!(result_rust, "All tests passed!");
    
    Ok(())
}
