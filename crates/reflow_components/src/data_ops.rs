//! Zeal Data Operations Actor
//!
//! Processes data operations defined in Zeal node templates with template literal support.

use std::collections::HashMap;
use actor_macro::actor;
use anyhow::{Result, Error};
use reflow_actor::{ActorContext, message::EncodableValue};
use serde_json::{json, Value};
use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};
use std::sync::Arc;
use reflow_actor::ActorConfig;
use reflow_tracing_protocol::client::TracingIntegration;
use rquickjs::{Runtime, Context as JsContext, Function as JsFunction};
use regex::Regex;

/// Data Operations Actor - Processes Zeal data operation sets
///
/// Handles the special ${input.get('portName').data.field} syntax used in Zeal data operations.
#[actor(
    DataOperationsActor,
    inports::<100>(data),
    outports::<50>(output, error),
    state(MemoryState)
)]
pub async fn data_operations_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    let config = context.get_config_hashmap();
    let payload = context.get_payload();
    
    // Get all input data from payload
    let all_inputs = payload.clone();
    
    // Get data operations from propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Look for dataOperations in propertyValues
    // The structure should be DataOperationSet[] with operations inside
    let operation_sets = property_values
        .and_then(|pv| pv.get("dataOperations"))
        .and_then(|v| v.as_array());
    
    if let Some(sets) = operation_sets {
        // Process each operation set
        for set in sets {
            let operations = set.get("operations")
                .and_then(|v| v.as_array());
            
            if let Some(ops) = operations {
                // Start with the initial data
                let mut current_data = if let Some(data) = all_inputs.get("data") {
                    serde_json::to_value(data)?
                } else {
                    // If no 'data' port, use all inputs
                    serde_json::to_value(&all_inputs)?
                };
                
                // Process each operation in sequence
                for operation in ops {
                    if !operation.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) {
                        continue; // Skip disabled operations
                    }
                    
                    let op_type = operation.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("transform");
                    
                    current_data = match op_type {
                        "map" => process_map_operation(current_data, operation, &all_inputs)?,
                        "filter" => process_filter_operation(current_data, operation, &all_inputs)?,
                        "sort" => process_sort_operation(current_data, operation, &all_inputs)?,
                        "transform" => process_transform_operation(current_data, operation, &all_inputs)?,
                        "group" => process_group_operation(current_data, operation, &all_inputs)?,
                        "aggregate" => process_aggregate_operation(current_data, operation, &all_inputs)?,
                        "merge" => process_merge_operation(current_data, operation, &all_inputs)?,
                        "split" => process_split_operation(current_data, operation, &all_inputs)?,
                        _ => current_data
                    };
                }
                
                result.insert("output".to_string(), json_value_to_message(current_data));
            }
        }
    } else {
        // No operations defined, pass through the data
        if let Some(data) = all_inputs.get("data") {
            result.insert("output".to_string(), data.clone());
        }
    }
    
    Ok(result)
}

/// Process template literals in expressions
/// Converts ${input.get('portName').data.field} to actual values
fn process_template_literals(expression: &str, inputs: &HashMap<String, Message>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}")?;
    let mut processed = expression.to_string();
    
    for cap in re.captures_iter(expression) {
        let full_match = &cap[0];
        let inner_expr = &cap[1];
        
        // Parse input.get('portName').data.field expressions
        if inner_expr.starts_with("input.get(") {
            if let Some(value) = extract_input_value(inner_expr, inputs) {
                let json_str = serde_json::to_string(&value)?;
                processed = processed.replace(full_match, &json_str);
            }
        }
    }
    
    Ok(processed)
}

/// Extract value from input.get('portName').data.field expression
fn extract_input_value(expr: &str, inputs: &HashMap<String, Message>) -> Option<Value> {
    // Parse input.get('portName') or input.get("portName")
    let port_re = Regex::new(r#"input\.get\(['"]([^'"]+)['"]\)"#).ok()?;
    
    if let Some(cap) = port_re.captures(expr) {
        let port_name = &cap[1];
        
        if let Some(port_data) = inputs.get(port_name) {
            let mut value = serde_json::to_value(port_data).ok()?;
            
            // Extract nested fields if present (.data.field.subfield)
            let rest = &expr[cap.get(0)?.end()..];
            if !rest.is_empty() {
                // Remove leading .data if present
                let field_path = if rest.starts_with(".data") {
                    &rest[5..]
                } else {
                    rest
                };
                
                // Navigate through nested fields
                for field in field_path.split('.').filter(|s| !s.is_empty()) {
                    // Handle array access like [0]
                    if field.starts_with('[') && field.ends_with(']') {
                        if let Ok(index) = field[1..field.len()-1].parse::<usize>() {
                            value = value.as_array()?.get(index)?.clone();
                        }
                    } else {
                        value = value.get(field)?.clone();
                    }
                }
            }
            
            return Some(value);
        }
    }
    
    None
}

fn process_map_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let mappings = operation.get("mapping")
        .and_then(|v| v.as_array());
    
    if let Some(mappings) = mappings {
        match data {
            Value::Array(items) => {
                let mapped_items: Result<Vec<Value>> = items.into_iter().map(|mut item| {
                    for mapping in mappings {
                        let source_field = mapping.get("sourceField")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let target_field = mapping.get("targetField")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let transform = mapping.get("transform")
                            .and_then(|v| v.as_str());
                        
                        if !source_field.is_empty() && !target_field.is_empty() {
                            // Process source field template
                            let processed_source = process_template_literals(source_field, inputs)?;
                            
                            // Apply transform if present
                            let value = if let Some(transform_expr) = transform {
                                if !transform_expr.is_empty() {
                                    let processed_transform = process_template_literals(transform_expr, inputs)?;
                                    evaluate_js_expression(&processed_transform, &item)?
                                } else {
                                    // Extract value from processed source
                                    serde_json::from_str(&processed_source)?
                                }
                            } else {
                                serde_json::from_str(&processed_source)?
                            };
                            
                            // Set the target field
                            if let Value::Object(ref mut obj) = item {
                                obj.insert(target_field.to_string(), value);
                            }
                        }
                    }
                    Ok(item)
                }).collect();
                
                Ok(Value::Array(mapped_items?))
            }
            _ => Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_filter_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let filter_expr = operation.get("filterExpression")
        .and_then(|v| v.as_str());
    
    if let Some(expr) = filter_expr {
        if !expr.is_empty() {
            let processed_expr = process_template_literals(expr, inputs)?;
            
            match data {
                Value::Array(items) => {
                    let filtered: Result<Vec<Value>> = items.into_iter()
                        .filter_map(|item| {
                            match evaluate_js_filter(&processed_expr, &item) {
                                Ok(true) => Some(Ok(item)),
                                Ok(false) => None,
                                Err(e) => Some(Err(e))
                            }
                        })
                        .collect();
                    Ok(Value::Array(filtered?))
                }
                _ => Ok(data)
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_sort_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let sort_field = operation.get("sortField")
        .and_then(|v| v.as_str());
    let sort_direction = operation.get("sortDirection")
        .and_then(|v| v.as_str())
        .unwrap_or("asc");
    
    if let Some(field_expr) = sort_field {
        if !field_expr.is_empty() {
            let processed_field = process_template_literals(field_expr, inputs)?;
            
            match data {
                Value::Array(mut items) => {
                    items.sort_by(|a, b| {
                        // Evaluate the field expression for each item
                        let a_val = evaluate_js_expression(&processed_field, a).ok();
                        let b_val = evaluate_js_expression(&processed_field, b).ok();
                        
                        let ordering = match (a_val, b_val) {
                            (Some(Value::Number(a)), Some(Value::Number(b))) => {
                                let a_f = a.as_f64().unwrap_or(0.0);
                                let b_f = b.as_f64().unwrap_or(0.0);
                                a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(&b),
                            _ => std::cmp::Ordering::Equal
                        };
                        
                        if sort_direction == "desc" {
                            ordering.reverse()
                        } else {
                            ordering
                        }
                    });
                    Ok(Value::Array(items))
                }
                _ => Ok(data)
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_transform_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let transform_expr = operation.get("transformExpression")
        .and_then(|v| v.as_str());
    
    if let Some(expr) = transform_expr {
        if !expr.is_empty() {
            let processed_expr = process_template_literals(expr, inputs)?;
            evaluate_js_expression(&processed_expr, &data)
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_group_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let group_field = operation.get("groupByField")
        .and_then(|v| v.as_str());
    
    if let Some(field_expr) = group_field {
        if !field_expr.is_empty() {
            let processed_field = process_template_literals(field_expr, inputs)?;
            
            match data {
                Value::Array(items) => {
                    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
                    
                    for item in items {
                        // Evaluate the field expression for this item
                        let key_value = evaluate_js_expression(&processed_field, &item)?;
                        let key = match key_value {
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => "undefined".to_string()
                        };
                        
                        groups.entry(key).or_insert_with(Vec::new).push(item);
                    }
                    
                    Ok(json!(groups))
                }
                _ => Ok(data)
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_aggregate_operation(data: Value, operation: &Value, inputs: &HashMap<String, Message>) -> Result<Value> {
    let agg_field = operation.get("aggregateField")
        .and_then(|v| v.as_str());
    let agg_function = operation.get("aggregateFunction")
        .and_then(|v| v.as_str())
        .unwrap_or("sum");
    
    match data {
        Value::Array(items) => {
            if let Some(field_expr) = agg_field {
                if !field_expr.is_empty() {
                    let processed_field = process_template_literals(field_expr, inputs)?;
                    
                    let values: Result<Vec<f64>> = items.iter()
                        .map(|item| {
                            let val = evaluate_js_expression(&processed_field, item)?;
                            match val {
                                Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
                                _ => Ok(0.0)
                            }
                        })
                        .collect();
                    
                    let values = values?;
                    let result = match agg_function {
                        "sum" => values.iter().sum::<f64>(),
                        "avg" => {
                            if values.is_empty() {
                                0.0
                            } else {
                                values.iter().sum::<f64>() / values.len() as f64
                            }
                        }
                        "count" => values.len() as f64,
                        "min" => values.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
                        "max" => values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
                        "first" => values.first().copied().unwrap_or(0.0),
                        "last" => values.last().copied().unwrap_or(0.0),
                        _ => 0.0
                    };
                    
                    Ok(json!({ agg_function: result }))
                } else {
                    // Count operation without field
                    Ok(json!({ "count": items.len() }))
                }
            } else {
                Ok(json!({ "count": items.len() }))
            }
        }
        _ => Ok(data)
    }
}

fn process_merge_operation(data: Value, _operation: &Value, _inputs: &HashMap<String, Message>) -> Result<Value> {
    // Merge would combine multiple data sources
    // For now, just return the data as-is
    Ok(data)
}

fn process_split_operation(data: Value, _operation: &Value, _inputs: &HashMap<String, Message>) -> Result<Value> {
    // Split would divide data into multiple outputs
    // For now, just return the data as-is
    Ok(data)
}

/// Evaluate JavaScript expression with context
fn evaluate_js_expression(expression: &str, context_data: &Value) -> Result<Value> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    ctx.with(|ctx| {
        // Set up the data context
        let globals = ctx.globals();
        let js_data: rquickjs::Value = ctx.json_parse(context_data.to_string())?;
        globals.set("data", js_data)?;
        
        // Wrap expression to return data context
        let wrapped_expr = format!("(function(data) {{ return {}; }})(data)", expression);
        
        // Evaluate the expression
        let js_result: rquickjs::Value = ctx.eval(wrapped_expr.as_str())?;
        
        // Convert result back to JSON
        let json_str = if let Some(s) = ctx.json_stringify(js_result)? {
            s.to_string()?
        } else {
            "null".to_string()
        };
        
        let result: Value = serde_json::from_str(&json_str)?;
        Ok(result)
    })
}

/// Evaluate JavaScript filter expression
fn evaluate_js_filter(expression: &str, item: &Value) -> Result<bool> {
    let runtime = Runtime::new()?;
    let ctx = JsContext::full(&runtime)?;
    
    ctx.with(|ctx| {
        // Set up the item context
        let globals = ctx.globals();
        let js_item: rquickjs::Value = ctx.json_parse(item.to_string())?;
        globals.set("item", js_item)?;
        
        // Wrap as a filter function
        let filter_fn = format!("(function(item) {{ return {}; }})(item)", expression);
        
        // Evaluate
        let result: bool = ctx.eval(filter_fn)?;
        
        Ok(result)
    })
}

// Helper function to convert JSON value to Message
fn json_value_to_message(value: Value) -> Message {
    match value {
        Value::Null => Message::Optional(None),
        Value::Bool(b) => Message::Boolean(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Message::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Message::Float(f)
            } else {
                Message::Float(0.0)
            }
        }
        Value::String(s) => Message::String(s.into()),
        Value::Array(arr) => {
            let items: Vec<EncodableValue> = arr.into_iter()
                .map(|v| v.into())
                .collect();
            Message::Array(items.into())
        }
        Value::Object(_) => Message::object(EncodableValue::from(value))
    }
}