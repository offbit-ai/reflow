//! Network-related Neon bindings
//! 
//! Provides Node.js bindings for Network and simplified GraphNetwork classes
//! with enhanced capabilities compared to WASM version

use neon::prelude::*;
use parking_lot::RwLock as ParkingRwLock;
use reflow_network::{network::{Network, NetworkConfig}};
use crate::runtime;
use std::sync::{Arc, Mutex};

/// Enhanced Node.js Network wrapper
pub struct NodeNetwork {
    basic: Arc<ParkingRwLock<Network>>,
}

impl Finalize for NodeNetwork {}

impl NodeNetwork {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeNetwork>> {
        let config = NetworkConfig::default();
        let network = Network::new(config);
        
        Ok(cx.boxed(NodeNetwork {
            basic: Arc::new(ParkingRwLock::new(network)),
        }))
    }

    fn start(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let basic = Arc::clone(&this.basic);
        
        // Execute synchronously for now to avoid complex async handling
        let result = runtime::execute_async(|| async move {
            let mut network = basic.write();
            network.start().await.map_err(|e| anyhow::Error::from(e))
        });
        
        match result {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(format!("Failed to start network: {}", e))
        }
    }

    fn stop(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let basic = Arc::clone(&this.basic);
        
        let network = basic.read();
        network.shutdown();
        
        Ok(cx.undefined())
    }

    fn shutdown(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let network = this.basic.read();
        network.shutdown();
        Ok(cx.undefined())
    }

    fn load_graph(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let graph_js = cx.argument::<JsValue>(0)?;
        
        // TODO: Convert JS graph to Rust Graph and load it
        // For now, return success
        Ok(cx.undefined())
    }

    fn send_message(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let target = cx.argument::<JsString>(0)?.value(&mut cx);
        let message_js = cx.argument::<JsValue>(1)?;
        
        // TODO: Convert JS message to Rust Message and send
        Ok(cx.undefined())
    }

    fn get_state(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let network = this.basic.read();
        
        // TODO: Convert network state to JS object
        Ok(cx.undefined().upcast())
    }

    fn is_running(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx.this::<JsBox<NodeNetwork>>()?;
        let network = this.basic.read();
        
        // TODO: Get actual running state
        Ok(cx.boolean(false))
    }
}

/// Simplified GraphNetwork wrapper for Node.js
pub struct NodeGraphNetwork {
    network: Arc<Mutex<Network>>,
}

impl Finalize for NodeGraphNetwork {}

impl NodeGraphNetwork {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphNetwork>> {
        // Create a basic network for graph operations
        let config = NetworkConfig::default();
        let network = Network::new(config);
        
        Ok(cx.boxed(NodeGraphNetwork {
            network: Arc::new(Mutex::new(network)),
        }))
    }

    fn start(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphNetwork>>()?;
        let network: Arc<Mutex<Network>> = Arc::clone(&this.network);
        
        let result = runtime::execute_async(|| async move {
            if let Ok(mut net) = network.lock() {
                net.start().await
            } else {
                Err(anyhow::Error::msg("Failed to lock network"))
            }
        });
        
        match result {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(format!("Failed to start graph network: {}", e))
        }
    }
}

/// Export function to create Network instances
pub fn create_network(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeNetwork::new)?)
}

/// Export function to create GraphNetwork instances  
pub fn create_graph_network(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphNetwork::new)?)
}
