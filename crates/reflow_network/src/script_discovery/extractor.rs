
use anyhow::{Result, Context};
use reflow_graph::types::PortType;
use serde_json;
use std::path::Path;
use std::process::Command;
use tracing::debug;
use super::types::*;


pub struct MetadataExtractor;

impl MetadataExtractor {
    pub fn new() -> Self {
        Self
    }
    
    /// Extract metadata from a Python actor file
    pub async fn extract_python_metadata(&self, file: &Path) -> Result<ExtractedMetadata> {
        debug!("Extracting Python metadata from: {}", file.display());
        
        // Python extraction script
        let extraction_script = r#"
import ast
import json
import sys
from pathlib import Path

def extract_actor_metadata(file_path):
    """Extract metadata from Python actor file."""
    with open(file_path, 'r') as f:
        tree = ast.parse(f.read())
    
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef) or isinstance(node, ast.AsyncFunctionDef):
            for decorator in node.decorator_list:
                if isinstance(decorator, ast.Call):
                    decorator_name = None
                    if hasattr(decorator.func, 'id'):
                        decorator_name = decorator.func.id
                    elif hasattr(decorator.func, 'attr'):
                        decorator_name = decorator.func.attr
                    
                    if decorator_name == 'actor':
                        return parse_actor_decorator(decorator, node)
    
    # Check for __actor_metadata__ variable
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == '__actor_metadata__':
                    return ast.literal_eval(node.value)
    
    return None

def parse_actor_decorator(decorator, func_node):
    """Parse @actor decorator arguments."""
    metadata = {
        'component': func_node.name,
        'description': ast.get_docstring(func_node) or '',
        'version': '1.0.0',
        'inports': [],
        'outports': [],
        'dependencies': [],
        'tags': [],
        'category': None,
        'config_schema': None
    }
    
    for keyword in decorator.keywords:
        if keyword.arg == 'name':
            metadata['component'] = ast.literal_eval(keyword.value)
        elif keyword.arg == 'inports':
            ports = ast.literal_eval(keyword.value)
            metadata['inports'] = [
                {
                    'name': p if isinstance(p, str) else p.get('name', ''),
                    'port_type': {'type': 'any'},
                    'required': True,
                    'description': '',
                    'default': None
                }
                for p in ports
            ]
        elif keyword.arg == 'outports':
            ports = ast.literal_eval(keyword.value)
            metadata['outports'] = [
                {
                    'name': p if isinstance(p, str) else p.get('name', ''),
                    'port_type': {'type': 'any'},
                    'required': False,
                    'description': '',
                    'default': None
                }
                for p in ports
            ]
        elif keyword.arg == 'version':
            metadata['version'] = ast.literal_eval(keyword.value)
        elif keyword.arg == 'description':
            metadata['description'] = ast.literal_eval(keyword.value)
        elif keyword.arg == 'tags':
            metadata['tags'] = ast.literal_eval(keyword.value)
        elif keyword.arg == 'category':
            metadata['category'] = ast.literal_eval(keyword.value)
    
    return metadata

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(json.dumps({'error': 'No file path provided'}))
        sys.exit(1)
    
    try:
        metadata = extract_actor_metadata(sys.argv[1])
        if metadata:
            print(json.dumps(metadata))
        else:
            print(json.dumps({'error': 'No actor metadata found'}))
    except Exception as e:
        print(json.dumps({'error': str(e)}))
        sys.exit(1)
"#;
        
        // Execute Python script
        let output = Command::new("python3")
            .arg("-c")
            .arg(extraction_script)
            .arg(file)
            .output()
            .context("Failed to execute Python extraction script")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Python extraction failed: {}", stderr));
        }
        
        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)
            .context("Failed to parse Python extraction output")?;
        
        if let Some(error) = result.get("error") {
            return Err(anyhow::anyhow!("Extraction error: {}", error));
        }
        
        self.parse_extracted_metadata(result)
    }
    
    /// Extract metadata from a JavaScript actor file
    pub async fn extract_javascript_metadata(&self, file: &Path) -> Result<ExtractedMetadata> {
        debug!("Extracting JavaScript metadata from: {}", file.display());
        
        // JavaScript extraction script
        let extraction_script = r#"
const fs = require('fs');
const path = require('path');

function extractActorMetadata(filePath) {
    const content = fs.readFileSync(filePath, 'utf8');
    
    // Look for @actor decorator pattern
    const decoratorRegex = /@actor\s*\(([\s\S]*?)\)/;
    const decoratorMatch = content.match(decoratorRegex);
    
    if (decoratorMatch) {
        try {
            // Parse decorator arguments
            const decoratorArgs = eval('(' + decoratorMatch[1] + ')');
            return parseDecoratorArgs(decoratorArgs, content);
        } catch (e) {
            // Try alternative parsing
        }
    }
    
    // Look for actorMetadata export
    const metadataRegex = /(?:export\s+)?(?:const|let|var)\s+actorMetadata\s*=\s*({[\s\S]*?});/;
    const metadataMatch = content.match(metadataRegex);
    
    if (metadataMatch) {
        try {
            return eval('(' + metadataMatch[1] + ')');
        } catch (e) {
            return { error: 'Failed to parse metadata: ' + e.message };
        }
    }
    
    // Look for __actor_metadata__ export
    const underscoreRegex = /(?:export\s+)?(?:const|let|var)\s+__actor_metadata__\s*=\s*({[\s\S]*?});/;
    const underscoreMatch = content.match(underscoreRegex);
    
    if (underscoreMatch) {
        try {
            return eval('(' + underscoreMatch[1] + ')');
        } catch (e) {
            return { error: 'Failed to parse metadata: ' + e.message };
        }
    }
    
    return { error: 'No actor metadata found' };
}

function parseDecoratorArgs(args, content) {
    // Extract function name
    const funcRegex = /function\s+(\w+)|const\s+(\w+)\s*=/;
    const funcMatch = content.match(funcRegex);
    const funcName = funcMatch ? (funcMatch[1] || funcMatch[2]) : 'unknown';
    
    // Extract description from JSDoc
    const jsdocRegex = /\/\*\*\s*\n?\s*\*\s*(.+?)\n/;
    const jsdocMatch = content.match(jsdocRegex);
    const description = jsdocMatch ? jsdocMatch[1].trim() : '';
    
    return {
        component: args.name || funcName,
        description: args.description || description,
        version: args.version || '1.0.0',
        inports: (args.inports || []).map(p => ({
            name: typeof p === 'string' ? p : p.name,
            port_type: { type: 'any' },
            required: true,
            description: '',
            default: null
        })),
        outports: (args.outports || []).map(p => ({
            name: typeof p === 'string' ? p : p.name,
            port_type: { type: 'any' },
            required: false,
            description: '',
            default: null
        })),
        dependencies: args.dependencies || [],
        tags: args.tags || [],
        category: args.category || null,
        config_schema: args.configSchema || null
    };
}

// Main execution
const filePath = process.argv[2];
if (!filePath) {
    console.log(JSON.stringify({ error: 'No file path provided' }));
    process.exit(1);
}

try {
    const metadata = extractActorMetadata(filePath);
    console.log(JSON.stringify(metadata));
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
    process.exit(1);
}
"#;
        
        // Execute Node.js script
        let output = Command::new("node")
            .arg("-e")
            .arg(extraction_script)
            .arg("--")
            .arg(file)
            .output()
            .context("Failed to execute JavaScript extraction script")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("JavaScript extraction failed: {}", stderr));
        }
        
        // Parse JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: serde_json::Value = serde_json::from_str(&stdout)
            .context("Failed to parse JavaScript extraction output")?;
        
        if let Some(error) = result.get("error") {
            return Err(anyhow::anyhow!("Extraction error: {}", error));
        }
        
        self.parse_extracted_metadata(result)
    }
    
    /// Parse extracted metadata from JSON
    fn parse_extracted_metadata(&self, json: serde_json::Value) -> Result<ExtractedMetadata> {
        // Parse port definitions
        let inports = self.parse_ports(json.get("inports"))?;
        let outports = self.parse_ports(json.get("outports"))?;
        
        Ok(ExtractedMetadata {
            component: json.get("component")
                .and_then(|v| v.as_str())
                .unwrap_or("UnknownActor")
                .to_string(),
            description: json.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version: json.get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0")
                .to_string(),
            inports,
            outports,
            dependencies: self.parse_string_array(json.get("dependencies"))?,
            config_schema: json.get("config_schema").cloned(),
            tags: self.parse_string_array(json.get("tags"))?,
            category: json.get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }
    
    /// Parse port definitions from JSON
    fn parse_ports(&self, value: Option<&serde_json::Value>) -> Result<Vec<PortDefinition>> {
        let Some(ports_value) = value else {
            return Ok(Vec::new());
        };
        
        let Some(ports_array) = ports_value.as_array() else {
            return Ok(Vec::new());
        };
        
        let mut ports = Vec::new();
        for port_value in ports_array {
            let port = if let Some(port_str) = port_value.as_str() {
                // Simple string port name
                PortDefinition {
                    name: port_str.to_string(),
                    port_type: PortType::Any,
                    required: true,
                    description: String::new(),
                    default: None,
                }
            } else if let Some(port_obj) = port_value.as_object() {
                // Detailed port definition
                PortDefinition {
                    name: port_obj.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    port_type: self.parse_port_type(port_obj.get("port_type"))?,
                    required: port_obj.get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    description: port_obj.get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    default: port_obj.get("default").cloned(),
                }
            } else {
                continue;
            };
            
            ports.push(port);
        }
        
        Ok(ports)
    }
    
    /// Parse port type from JSON
    fn parse_port_type(&self, value: Option<&serde_json::Value>) -> Result<PortType> {
        let Some(type_value) = value else {
            return Ok(PortType::Any);
        };
        
        if let Some(type_str) = type_value.as_str() {
            // Simple string type
            match type_str {
                "any" => Ok(PortType::Any),
                "flow" => Ok(PortType::Flow),
                "integer" => Ok(PortType::Integer),
                "float" => Ok(PortType::Float),
                "string" => Ok(PortType::String),
                "boolean" => Ok(PortType::Boolean),
                "stream" => Ok(PortType::Stream),
                _ => Ok(PortType::Any),
            }
        } else if let Some(type_obj) = type_value.as_object() {
            // Complex type object
            if let Some(type_name) = type_obj.get("type").and_then(|v| v.as_str()) {
                match type_name {
                    "any" => Ok(PortType::Any),
                    "flow" => Ok(PortType::Flow),
                    "integer" => Ok(PortType::Integer),
                    "float" => Ok(PortType::Float),
                    "string" => Ok(PortType::String),
                    "boolean" => Ok(PortType::Boolean),
                    "stream" => Ok(PortType::Stream),
                    "array" => {
                        // TODO: Parse nested array type
                        Ok(PortType::Array(Box::new(PortType::Any)))
                    }
                    "object" => {
                        let schema = type_obj.get("value")
                            .and_then(|v| v.as_str())
                            .unwrap_or("{}");
                        Ok(PortType::Object(schema.to_string()))
                    }
                    _ => Ok(PortType::Any),
                }
            } else {
                Ok(PortType::Any)
            }
        } else {
            Ok(PortType::Any)
        }
    }
    
    /// Parse string array from JSON
    fn parse_string_array(&self, value: Option<&serde_json::Value>) -> Result<Vec<String>> {
        let Some(array_value) = value else {
            return Ok(Vec::new());
        };
        
        let Some(array) = array_value.as_array() else {
            return Ok(Vec::new());
        };
        
        Ok(array.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect())
    }
}