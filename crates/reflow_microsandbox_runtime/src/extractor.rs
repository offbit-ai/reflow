use serde_json::{json, Value};
use anyhow::Result;
use reflow_network::script_discovery::types::ScriptRuntime;
use regex::Regex;

/// Extract metadata from script source based on decorators or comments
pub fn extract_metadata(source: &str, runtime: ScriptRuntime) -> Result<Value> {
    match runtime {
        ScriptRuntime::Python => extract_python_metadata(source),
        ScriptRuntime::JavaScript => extract_javascript_metadata(source),
        _ => Ok(json!({
            "component": "unknown",
            "description": "No metadata found",
            "inports": [],
            "outports": []
        }))
    }
}

/// Extract metadata from Python script using @actor decorator
fn extract_python_metadata(source: &str) -> Result<Value> {
    // Look for @actor decorator with metadata
    // Pattern: @actor({...metadata...})
    let decorator_regex = Regex::new(r"@actor\s*\(\s*(\{[^}]+\})\s*\)")?;
    
    if let Some(captures) = decorator_regex.captures(source) {
        if let Some(metadata_str) = captures.get(1) {
            // Convert Python dict syntax to JSON
            let json_str = metadata_str.as_str()
                .replace("'", "\"")  // Single quotes to double quotes
                .replace("True", "true")
                .replace("False", "false")
                .replace("None", "null");
            
            // Try to parse as JSON
            if let Ok(metadata) = serde_json::from_str::<Value>(&json_str) {
                return Ok(metadata);
            }
        }
    }
    
    // If no @actor decorator found, return an error
    Err(anyhow::anyhow!("No @actor decorator found in Python script"))
}

/// Extract metadata from JavaScript/Node.js script
fn extract_javascript_metadata(source: &str) -> Result<Value> {
    // Look for metadata object or comment
    // Pattern: const metadata = {...} or /* @metadata {...} */
    let metadata_regex = Regex::new(r"(?:const|let|var)\s+metadata\s*=\s*(\{[^}]+\})")?;
    
    if let Some(captures) = metadata_regex.captures(source) {
        if let Some(metadata_str) = captures.get(1) {
            // JavaScript object literal should be valid JSON
            if let Ok(metadata) = serde_json::from_str::<Value>(metadata_str.as_str()) {
                return Ok(metadata);
            }
        }
    }
    
    // Look for JSDoc-style metadata
    let jsdoc_regex = Regex::new(r"/\*\*\s*\n\s*\*\s*@component\s+(\w+)\s*\n\s*\*\s*@description\s+([^\n]+)")?;
    
    if let Some(captures) = jsdoc_regex.captures(source) {
        let name = captures.get(1).map(|m| m.as_str()).unwrap_or("unknown");
        let description = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        
        return Ok(json!({
            "component": name,
            "description": description,
            "inports": ["input"],
            "outports": ["output"]
        }));
    }
    
    // Fallback: Look for function name
    let func_regex = Regex::new(r"(?:async\s+)?function\s+(\w+)\s*\([^)]*\)")?;
    
    if let Some(captures) = func_regex.captures(source) {
        let name = captures.get(1).map(|m| m.as_str()).unwrap_or("unknown");
        
        return Ok(json!({
            "component": name,
            "description": "JavaScript actor",
            "inports": ["input"],
            "outports": ["output"]
        }));
    }
    
    Ok(json!({
        "component": "js_actor",
        "description": "JavaScript actor",
        "inports": ["input"],
        "outports": ["output"]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_python_metadata_with_decorator() {
        let source = r#"
import json

@actor({
    "component": "test_multiplier",
    "description": "Multiplies input by 2",
    "inports": ["input"],
    "outports": ["output"]
})
async def process(context):
    pass
"#;
        
        let metadata = extract_python_metadata(source).unwrap();
        assert_eq!(metadata["component"], "test_multiplier");
        assert_eq!(metadata["description"], "Multiplies input by 2");
    }
    
    #[test]
    fn test_extract_javascript_metadata_with_const() {
        let source = r#"
const metadata = {
    "component": "data_processor",
    "description": "Processes data",
    "inports": ["data"],
    "outports": ["result"]
};

async function process(context) {
    // ...
}
"#;
        
        let metadata = extract_javascript_metadata(source).unwrap();
        assert_eq!(metadata["component"], "data_processor");
        assert_eq!(metadata["description"], "Processes data");
    }
}