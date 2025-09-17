//! Zeal Template to Actor Registry
//!
//! Maps Zeal template IDs to their corresponding actor implementations.

use std::collections::HashMap;
use std::sync::Arc;
use crate::Actor;

// Import all actor types
use crate::integration::{
    HttpRequestActor, PostgreSQLPoolActor, MySQLPoolActor, MongoDbPoolActor, MongoCollectionActor
};
use crate::flow_control::{
    ConditionalBranchActor, SwitchCaseActor, LoopActor
};
use crate::data_operations::{
    DataTransformActor
};
use crate::scripting::{
    JavaScriptScriptActor, SqlScriptActor, PythonScriptActor
};
use crate::data_ops::{
    DataOperationsActor
};
use crate::rules::{
    RulesEngineActor
};

/// Get an actor instance for a given Zeal template ID
pub fn get_actor_for_template(template_id: &str) -> Option<Arc<dyn Actor>> {
    match template_id {
        // Tools/Utilities - HTTP
        "tpl_http_request" => {
            Some(Arc::new(HttpRequestActor::new()))
        }
        
        // Data Sources - Databases (Connection Pools)
        "tpl_mysql" => {
            Some(Arc::new(MySQLPoolActor::new()))
        }
        
        "tpl_postgresql" => {
            Some(Arc::new(PostgreSQLPoolActor::new()))
        }
        
        "tpl_mongodb" => {
            Some(Arc::new(MongoDbPoolActor::new()))
        }
        
        // MongoDB Collection Operations
        "tpl_mongo_get_collection" => {
            Some(Arc::new(MongoCollectionActor::new()))
        }
        
        // Logic Control
        "tpl_if_branch" => {
            Some(Arc::new(ConditionalBranchActor::new()))
        }
        
        "tpl_switch" => {
            Some(Arc::new(SwitchCaseActor::new()))
        }
        
        "tpl_loop" => {
            Some(Arc::new(LoopActor::new()))
        }
        
        // Data Processing
        "tpl_data_transformer" => {
            Some(Arc::new(DataTransformActor::new()))
        }
        
        // Scripting
        "tpl_javascript_script" => {
            Some(Arc::new(JavaScriptScriptActor::new()))
        }
        
        "tpl_sql_script" => {
            Some(Arc::new(SqlScriptActor::new()))
        }
        
        "tpl_python_script" => {
            Some(Arc::new(PythonScriptActor::new()))
        }
        
        _ => None
    }
}

/// Get the complete mapping of template IDs to actor names
pub fn get_template_mapping() -> HashMap<String, String> {
    let mut mapping = HashMap::new();
    
    // Tools/Utilities mappings
    mapping.insert("tpl_http_request".to_string(), "HttpRequestActor".to_string());
    
    // Data Sources mappings
    mapping.insert("tpl_mysql".to_string(), "MySQLPoolActor".to_string());
    mapping.insert("tpl_postgresql".to_string(), "PostgreSQLPoolActor".to_string());
    mapping.insert("tpl_mongodb".to_string(), "MongoDbPoolActor".to_string());
    
    // Logic Control mappings
    mapping.insert("tpl_if_branch".to_string(), "ConditionalBranchActor".to_string());
    mapping.insert("tpl_switch".to_string(), "SwitchCaseActor".to_string());
    mapping.insert("tpl_loop".to_string(), "LoopActor".to_string());
    
    // Data Processing mappings
    mapping.insert("tpl_data_transformer".to_string(), "DataTransformActor".to_string());
    
    mapping
}