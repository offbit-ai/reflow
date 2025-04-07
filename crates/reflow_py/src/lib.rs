use anyhow::Result;
use core::time;
use tracing::{error, info};
use pyexec_service::{
    error::ServiceError,
    package_manager,
    python_vm::{ExecutionResult, Interpreter, MESSAGE_VALUE_SENDERS},
};
use remote_py::ClientConfig;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::http::status;

mod remote_py;
mod remote_tests;
mod tests;

#[derive(Clone)]
pub struct PythonRuntime {
    use_remote: bool,
    use_shared_env: bool,
    remote_client: Option<Arc<Mutex<remote_py::PyExecClient>>>,
    interpreter: Arc<Mutex<Interpreter>>,
    requirements: Option<Vec<String>>,
    reciever: Option<Arc<Mutex<mpsc::UnboundedReceiver<serde_json::Value>>>>,
}

impl PythonRuntime {
    pub fn new(use_remote: bool, use_shared_env: bool) -> Self {
        Self {
            use_remote,
            use_shared_env,
            remote_client: None,
            interpreter: Arc::new(Mutex::new(Interpreter::new())),
            requirements: None,
            reciever: None,
        }
    }

    pub async fn init(
        &mut self,
        session_id: uuid::Uuid,
        requirements: Option<Vec<String>>,
        config: Option<ClientConfig>,
    ) -> Result<()> {
        self.requirements = requirements;
        let (tx, rx) = mpsc::unbounded_channel::<serde_json::Value>();
        self.reciever = Some(Arc::new(Mutex::new(rx)));

        if self.use_remote {
            let config = config.unwrap_or_default();
            self.init_remote(Some(config)).await?;
        } else {
            self.interpreter = pyexec_service::python_vm::register_session(session_id);
            MESSAGE_VALUE_SENDERS.insert(session_id, tx);
            pyexec_service::package_manager::set_use_shared_environment(self.use_shared_env);
            pyexec_service::package_manager::initialize_venv(&session_id)?;
            let (progress_sender, mut progress_reciever) = tokio::sync::mpsc::unbounded_channel();
            let requirements = self.requirements.as_ref();

            pyexec_service::package_manager::install_packages(
                &session_id,
                requirements.unwrap_or(&vec![]),
                Some(Arc::new(Mutex::new(progress_sender))),
            )
            .await?;

            while let Some(message) = progress_reciever.recv().await {
                let value = serde_json::from_str::<serde_json::Value>(&message)?;
                if let Some(status) = value.get("status") {
                    if status.to_string() == "complete" {
                        break;
                    }
                    info!(
                        "Python package installation progress: {}",
                        status.to_string()
                    );
                }
            }
        }
        Ok(())
    }

    pub async fn init_remote(&mut self, config: Option<ClientConfig>) -> Result<()> {
        let client = remote_py::PyExecClient::create_without_waiting(
            &std::env::var("PYEXEC_SERVICE_URL").unwrap_or("ws://0.0.0.0:8080".to_string()),
            config,
        )
        .await;
        self.remote_client = Some(Arc::new(Mutex::new(client)));
        Ok(())
    }

    pub async fn execute(
        &self,
        session_id: uuid::Uuid,
        code: &str,
        inputs: HashMap<String, serde_json::Value>,
        timeout_duration: Option<time::Duration>,
        message_rx: Option<flume::Sender<serde_json::Value>>,
    ) -> Result<ExecutionResult, ServiceError> {
        if self.use_remote {
            let remote_guard = self
                .remote_client
                .clone()
                .expect("No remote client instance provided for python script execution");
            let remote_client = remote_guard.lock().await;
            let input_params = inputs
                .iter()
                .map(|(k, v)| remote_py::InputParameter {
                    name: k.to_string(),
                    data: v.clone(),
                    description: None,
                })
                .collect::<Vec<_>>();

            let options = remote_py::ExecutionOptions {
                packages: self.requirements.clone(),
                inputs: Some(input_params),
                timeout: timeout_duration.map(|d| d.as_secs() as u64),
            };

            // Create message callback if message_rx is provided
            let message_callback = message_rx.map(|tx| {
                let tx = tx.clone();
                move |msg: serde_json::Value| {
                    if let Err(e) = tx.send(msg) {
                        error!("Failed to send client message: {}", e);
                    }
                }
            });

            match remote_client
                .execute(code, Some(options), message_callback)
                .await
            {
                Ok(result) => Ok(ExecutionResult {
                    stdout: result.stdout,
                    stderr: result.stderr,
                    result: result.result,
                    success: result.success,
                    execution_time: time::Duration::from_secs(result.execution_time_ms),
                    packages_installed: result.packages_installed,
                }),
                Err(e) => Err(ServiceError::Internal(e.to_string())),
            }
        } else {
            let inputs = inputs
                .iter()
                .map(|(k, v)| pyexec_service::rpc::InputParamRequest {
                    name: k.to_string(),
                    data: v.clone(),
                    description: None,
                })
                .collect();

            // Create a channel for package installation progress updates
            let (progress_sender, _progress_receiver) = mpsc::unbounded_channel::<String>();
            let msg_sender_clone = MESSAGE_VALUE_SENDERS.get(&session_id).map(|s| s.clone());
            let _message_rx = message_rx.clone();
            if let Some(sender) = msg_sender_clone {
                // Forward package installation progress to the client
                let mut progress_receiver = _progress_receiver;
                tokio::spawn(async move {
                    while let Some(progress) = progress_receiver.recv().await {
                        let progress_message = serde_json::to_value(progress)
                            .expect("Failed to parse progress message");
                        let _ = sender.send(progress_message.clone()).unwrap();
                    }
                });
            }

            // Forward messages from the interpreter to the client
            let message_rx = message_rx.clone();
            if let Some(reciever) = self.reciever.clone() {
                let reciever = reciever.clone();
                tokio::spawn(async move {
                    while let Some(msg) = reciever.lock().await.recv().await {
                        if let Some(sender) = message_rx.clone() {
                            sender.send(msg).unwrap();
                        }
                    }
                });
            }

            // Install required packages if specified
            let mut installed_packages = Vec::new();
            if let Some(reqs) = &self.requirements {
                if !reqs.is_empty() {
                    // Initialize the virtual environment and install packages
                    match package_manager::install_packages(
                        &session_id,
                        reqs,
                        Some(Arc::new(Mutex::new(progress_sender))),
                    )
                    .await
                    {
                        Ok(_) => {
                            installed_packages = reqs.clone();
                            info!("Successfully installed packages: {:?}", installed_packages);
                        }
                        Err(e) => {
                            error!("Failed to install packages: {}", e);
                            return Err(ServiceError::Internal(format!(
                                "Failed to install packages: {}",
                                e
                            )));
                        }
                    }
                }
            }

            let interpreter = self.interpreter.clone();
            let code = code.to_string();

            let execution = tokio::task::spawn_blocking(move || {
                let result =
                    futures::executor::block_on(pyexec_service::python_vm::execute_script(
                        &session_id,
                        code.as_str(),
                        Some(inputs),
                        interpreter.clone(),
                    ));

                match result {
                    Ok(mut inner_result) => {
                        inner_result.packages_installed = installed_packages;
                        Ok(inner_result)
                    }
                    Err(e) => Err(ServiceError::Internal(format!(
                        "Script execution failed: {}",
                        e
                    ))),
                }
            });

            match tokio::time::timeout(
                timeout_duration.unwrap_or(time::Duration::from_secs(10)),
                execution,
            )
            .await
            {
                Ok(result) => match result {
                    Ok(inner_result) => Ok(inner_result?),
                    Err(e) => Err(ServiceError::Internal(format!("Task panicked: {}", e))),
                },
                Err(_) => {
                    // Timeout occurred
                    Err(ServiceError::Internal(format!(
                        "Script execution timed out after {} seconds",
                        timeout_duration
                            .unwrap_or(time::Duration::from_secs(30))
                            .as_secs()
                    )))
                }
            }
        }
    }

    pub fn cleanup(&mut self) -> Result<()> {
        pyexec_service::python_vm::INTERPRETERS
            .iter()
            .for_each(|v| {
                let session_id = v.key();
                let _ = package_manager::cleanup_venv(session_id).expect("Failed to cleanup venv");
            });
        pyexec_service::python_vm::INTERPRETERS.clear();
        pyexec_service::python_vm::MESSAGE_SENDERS.clear();
        pyexec_service::python_vm::MESSAGE_VALUE_SENDERS.clear();

        Ok(())
    }
}
