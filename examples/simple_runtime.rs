use reflow_js::{JsRuntime, RuntimeConfig, ConsoleModule, TimerModule, FileSystemModule, FileSystemPermissions, FetchModule, NetworkConfig};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a runtime configuration
    let config = RuntimeConfig {
        enable_console: true,
        enable_timers: true,
        enable_filesystem: true,
        enable_network: true,
        enable_modules: true,
        ..Default::default()
    };
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Evaluate JavaScript code
    let code = r#"
        console.log("Hello, world!");
        
        // Create a file
        const fs = require('fs');
        fs.writeFileSync('hello.txt', 'Hello from JavaScript!');
        
        // Read the file
        const content = fs.readFileSync('hello.txt', 'utf8');
        console.log(`File content: ${content}`);
        
        // Use setTimeout
        setTimeout(() => {
            console.log("This message is logged after 1 second");
            
            // Use fetch
            fetch('https://jsonplaceholder.typicode.com/todos/1')
                .then(response => response.json())
                .then(json => console.log(`Fetch result: ${JSON.stringify(json)}`))
                .catch(error => console.error(`Fetch error: ${error}`));
        }, 1000);
        
        // Return a value
        42
    "#;
    
    let result = runtime.eval(code)?;
    println!("Result: {}", result);
    
    // Wait for the timer to fire and the fetch to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    Ok(())
}
