use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Redis state operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateOperation {
    Get { key: String },
    Set { key: String, value: Value },
    Delete { key: String },
    Increment { key: String, amount: i64 },
    Decrement { key: String, amount: i64 },
    Push { key: String, value: Value },
    Pop { key: String },
    Extend { key: String, values: Vec<Value> },
    Expire { key: String, seconds: i64 },
    Ttl { key: String },
    Exists { key: String },
    Keys { pattern: String },
}

/// Result of a state operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateResult {
    Value(Option<Value>),
    Number(i64),
    Boolean(bool),
    List(Vec<String>),
    Error(String),
}