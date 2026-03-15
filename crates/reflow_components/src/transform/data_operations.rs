//! Advanced data operations actor with template literal support and JS expression evaluation.

use super::data_transform::json_value_to_message;
use super::js_eval::{
    evaluate_js_expression_with_inputs, evaluate_js_filter_with_inputs, resolve_template_string,
};
use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Data Operations Actor - Processes Zeal data operation sets
///
/// Handles the special `${input.get('portName').data.field}` syntax used in Zeal data operations.
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

    let all_inputs = payload.clone();

    let operation_sets = config
        .get("dataOperations")
        .and_then(|v| v.as_array());

    if let Some(sets) = operation_sets {
        for set in sets {
            let operations = set.get("operations").and_then(|v| v.as_array());

            if let Some(ops) = operations {
                let mut current_data = if let Some(data) = all_inputs.get("data") {
                    serde_json::to_value(data)?
                } else {
                    serde_json::to_value(&all_inputs)?
                };

                for operation in ops {
                    if !operation
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true)
                    {
                        continue;
                    }

                    let op_type = operation
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("transform");

                    current_data = match op_type {
                        "map" => process_map_operation(current_data, operation, &all_inputs)?,
                        "filter" => process_filter_operation(current_data, operation, &all_inputs)?,
                        "sort" => process_sort_operation(current_data, operation, &all_inputs)?,
                        "transform" => {
                            process_transform_operation(current_data, operation, &all_inputs)?
                        }
                        "group" => process_group_operation(current_data, operation, &all_inputs)?,
                        "aggregate" => {
                            process_aggregate_operation(current_data, operation, &all_inputs)?
                        }
                        _ => current_data,
                    };
                }

                result.insert("output".to_string(), json_value_to_message(current_data));
            }
        }
    } else {
        // No operations defined, pass through
        if let Some(data) = all_inputs.get("data") {
            result.insert("output".to_string(), data.clone());
        }
    }

    Ok(result)
}

/// Resolve template strings via JS evaluation.
/// Handles `${input.get('portName').data.field}` and any valid JS template expression.
fn resolve_templates(
    expression: &str,
    data: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<String> {
    resolve_template_string(expression, data, inputs)
}

fn process_map_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let mappings = operation.get("mapping").and_then(|v| v.as_array());

    if let Some(mappings) = mappings {
        match data {
            Value::Array(items) => {
                let mapped_items: Result<Vec<Value>> = items
                    .into_iter()
                    .map(|mut item| {
                        for mapping in mappings {
                            let source_field = mapping
                                .get("sourceField")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let target_field = mapping
                                .get("targetField")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let transform = mapping.get("transform").and_then(|v| v.as_str());

                            if !source_field.is_empty() && !target_field.is_empty() {
                                let value = if let Some(transform_expr) = transform {
                                    if !transform_expr.is_empty() {
                                        let resolved =
                                            resolve_templates(transform_expr, &item, inputs)?;
                                        evaluate_js_expression_with_inputs(
                                            &resolved,
                                            &item,
                                            Some(inputs),
                                        )?
                                    } else {
                                        let resolved =
                                            resolve_templates(source_field, &item, inputs)?;
                                        serde_json::from_str(&resolved)?
                                    }
                                } else {
                                    let resolved = resolve_templates(source_field, &item, inputs)?;
                                    serde_json::from_str(&resolved)?
                                };

                                if let Value::Object(ref mut obj) = item {
                                    obj.insert(target_field.to_string(), value);
                                }
                            }
                        }
                        Ok(item)
                    })
                    .collect();

                Ok(Value::Array(mapped_items?))
            }
            _ => Ok(data),
        }
    } else {
        Ok(data)
    }
}

fn process_filter_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let filter_expr = operation.get("filterExpression").and_then(|v| v.as_str());

    if let Some(expr) = filter_expr {
        if !expr.is_empty() {
            let resolved_expr = resolve_templates(expr, &data, inputs)?;

            match data {
                Value::Array(items) => {
                    let filtered: Result<Vec<Value>> = items
                        .into_iter()
                        .filter_map(|item| {
                            match evaluate_js_filter_with_inputs(
                                &resolved_expr,
                                &item,
                                Some(inputs),
                            ) {
                                Ok(true) => Some(Ok(item)),
                                Ok(false) => None,
                                Err(e) => Some(Err(e)),
                            }
                        })
                        .collect();
                    Ok(Value::Array(filtered?))
                }
                _ => Ok(data),
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_sort_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let sort_field = operation.get("sortField").and_then(|v| v.as_str());
    let sort_direction = operation
        .get("sortDirection")
        .and_then(|v| v.as_str())
        .unwrap_or("asc");

    if let Some(field_expr) = sort_field {
        if !field_expr.is_empty() {
            let resolved_field = resolve_templates(field_expr, &data, inputs)?;

            match data {
                Value::Array(mut items) => {
                    items.sort_by(|a, b| {
                        let a_val =
                            evaluate_js_expression_with_inputs(&resolved_field, a, Some(inputs))
                                .ok();
                        let b_val =
                            evaluate_js_expression_with_inputs(&resolved_field, b, Some(inputs))
                                .ok();

                        let ordering = match (a_val, b_val) {
                            (Some(Value::Number(a)), Some(Value::Number(b))) => {
                                let a_f = a.as_f64().unwrap_or(0.0);
                                let b_f = b.as_f64().unwrap_or(0.0);
                                a_f.partial_cmp(&b_f).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(&b),
                            _ => std::cmp::Ordering::Equal,
                        };

                        if sort_direction == "desc" {
                            ordering.reverse()
                        } else {
                            ordering
                        }
                    });
                    Ok(Value::Array(items))
                }
                _ => Ok(data),
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_transform_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let transform_expr = operation
        .get("transformExpression")
        .and_then(|v| v.as_str());

    if let Some(expr) = transform_expr {
        if !expr.is_empty() {
            let resolved_expr = resolve_templates(expr, &data, inputs)?;
            evaluate_js_expression_with_inputs(&resolved_expr, &data, Some(inputs))
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_group_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let group_field = operation.get("groupByField").and_then(|v| v.as_str());

    if let Some(field_expr) = group_field {
        if !field_expr.is_empty() {
            let resolved_field = resolve_templates(field_expr, &data, inputs)?;

            match data {
                Value::Array(items) => {
                    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();

                    for item in items {
                        let key_value = evaluate_js_expression_with_inputs(
                            &resolved_field,
                            &item,
                            Some(inputs),
                        )?;
                        let key = match key_value {
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => "undefined".to_string(),
                        };

                        groups.entry(key).or_default().push(item);
                    }

                    Ok(json!(groups))
                }
                _ => Ok(data),
            }
        } else {
            Ok(data)
        }
    } else {
        Ok(data)
    }
}

fn process_aggregate_operation(
    data: Value,
    operation: &Value,
    inputs: &HashMap<String, Message>,
) -> Result<Value> {
    let agg_field = operation.get("aggregateField").and_then(|v| v.as_str());
    let agg_function = operation
        .get("aggregateFunction")
        .and_then(|v| v.as_str())
        .unwrap_or("sum");

    match data {
        Value::Array(items) => {
            if let Some(field_expr) = agg_field {
                if !field_expr.is_empty() {
                    let resolved_field =
                        resolve_templates(field_expr, &Value::Array(items.clone()), inputs)?;

                    let values: Result<Vec<f64>> = items
                        .iter()
                        .map(|item| {
                            let val = evaluate_js_expression_with_inputs(
                                &resolved_field,
                                item,
                                Some(inputs),
                            )?;
                            match val {
                                Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
                                _ => Ok(0.0),
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
                        _ => 0.0,
                    };

                    Ok(json!({ agg_function: result }))
                } else {
                    Ok(json!({ "count": items.len() }))
                }
            } else {
                Ok(json!({ "count": items.len() }))
            }
        }
        _ => Ok(data),
    }
}
