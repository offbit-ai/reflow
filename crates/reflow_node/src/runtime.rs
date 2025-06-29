//! Tokio runtime management for async operations in Neon
//! 
//! Provides a shared tokio runtime for all async operations in the Node.js bindings.
//! This allows us to use full native async capabilities instead of WASM limitations.

use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::runtime::Runtime;

static RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global tokio runtime
pub fn init_runtime() {
    RUNTIME.get_or_init(|| {
        let rt = Runtime::new().expect("Failed to create tokio runtime");
        Arc::new(rt)
    });
}

/// Get the global tokio runtime
pub fn get_runtime() -> &'static Arc<Runtime> {
    RUNTIME.get().expect("Runtime not initialized")
}

/// Execute an async function on the tokio runtime and block until completion
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    get_runtime().block_on(future)
}

/// Spawn a task on the tokio runtime
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    get_runtime().spawn(future)
}

/// Execute an async closure and return the result synchronously
/// This is a simplified approach that avoids the complex Neon task API
pub fn execute_async<F, Fut, T, E>(future_fn: F) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let future = future_fn();
    get_runtime().block_on(future)
}
