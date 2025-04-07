use crate::error::ServiceError;
use crate::package_manager;
use crate::rpc::InputParamRequest;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::types::*;
use pyo3::Python;
use pyo3::{prelude::*, PyNativeType};
use serde_json::Value as JsonValue;
use std::io::{self, Write};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{error, info};
use uuid::Uuid;

// Global store for interpreters, keyed by session ID
pub static INTERPRETERS: Lazy<DashMap<Uuid, Arc<Mutex<Interpreter>>>> = Lazy::new(DashMap::new);

// Global store for message senders, keyed by session ID
pub static MESSAGE_SENDERS: Lazy<DashMap<Uuid, mpsc::UnboundedSender<String>>> =
    Lazy::new(DashMap::new);

pub static MESSAGE_VALUE_SENDERS: Lazy<DashMap<Uuid, mpsc::UnboundedSender<serde_json::Value>>> =
    Lazy::new(DashMap::new);

// Default timeout for Python script execution (in seconds)
const DEFAULT_EXECUTION_TIMEOUT: u64 = 30;

pub struct Interpreter {
    pub stdout_capture: Vec<u8>,
    pub stderr_capture: Vec<u8>,
    pub globals: Option<PyObject>,
}

impl Interpreter {
    pub fn new() -> Self {
        let globals = Python::with_gil(|py| {
            let globals = PyDict::new(py);
            // Store globals as PyObject to keep them between calls
            globals.to_object(py)
        });

        Self {
            stdout_capture: Vec::new(),
            stderr_capture: Vec::new(),
            globals: Some(globals),
        }
    }

    pub fn reset_buffers(&mut self) {
        self.stdout_capture.clear();
        self.stderr_capture.clear();
    }
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub result: Option<JsonValue>,
    pub success: bool,
    pub execution_time: Duration,
    pub packages_installed: Vec<String>,
}

#[pyclass]
pub struct Messenger {
    pub session_id: String,
}

#[pymethods]
impl Messenger {
    #[new]
    fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
        }
    }

    fn send_message<'a>(&self, message: &'a PyAny) -> PyResult<bool> {
        match Uuid::from_str(&self.session_id) {
            Ok(uuid) => {
                if let Some(sender) = MESSAGE_SENDERS.get(&uuid) {
                    if let Ok(value) = py_to_json(self.py(), message) {
                        return match sender.send(
                            serde_json::to_string(&value).expect("Failed to serialize message"),
                        ) {
                            Ok(_) => Ok(true),
                            Err(e) => {
                                let err_msg = format!("Error sending message: {}", e);
                                eprintln!("{}", err_msg);
                                Ok(false)
                            }
                        };
                    } else {
                        eprintln!("Failed to parse message for session: {}", self.session_id);
                        return Ok(false);
                    }
                }

                if let Some(sender) = MESSAGE_VALUE_SENDERS.get(&uuid) {
                    if let Ok(value) = py_to_json(self.py(), message) {
                        return match sender.send(value) {
                            Ok(_) => Ok(true),
                            Err(e) => {
                                let err_msg = format!("Error sending message: {}", e);
                                eprintln!("{}", err_msg);
                                Ok(false)
                            }
                        };
                    } else {
                        eprintln!("Failed to parse message for session: {}", self.session_id);
                        return Ok(false);
                    }
                }

                eprintln!("No message sender for session: {}", self.session_id);
                Ok(false)
            }
            Err(e) => {
                eprintln!("Invalid session ID: {} - {}", self.session_id, e);
                Ok(false)
            }
        }
    }
}

impl ToPyObject for Messenger {
    fn to_object(&self, py: Python) -> PyObject {
        let obj = PyModule::new(py, "MessengerModule").unwrap();
        obj.add_class::<Messenger>()
            .expect("expected to add Messenger class");

        let messenger_instance = Py::new(py, Messenger::new(&self.session_id))
            .expect("Failed to create Messenger instance");

        messenger_instance.into_py(py)
    }
}

unsafe impl PyNativeType for Messenger {}

// Python class to store input parameters
#[pyclass]
#[derive(Clone)]
pub struct PyInputParameter {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub data: PyObject,
    #[pyo3(get)]
    pub description: Option<String>,
}

#[pymethods]
impl PyInputParameter {
    #[new]
    fn new(name: String, data: PyObject, description: Option<String>) -> Self {
        Self {
            name,
            data,
            description,
        }
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "InputParameter(name={}, description={})",
            self.name,
            self.description
                .clone()
                .unwrap_or_else(|| "None".to_string())
        ))
    }
}

impl ToPyObject for PyInputParameter {
    fn to_object(&self, py: Python) -> PyObject {
        let obj = PyModule::new(py, "InputParameterModule").unwrap();
        obj.add_class::<PyInputParameter>()
            .expect("expected to add PyInputParameter class");

        let messenger_instance = Py::new(
            py,
            Self::new(
                self.name.clone(),
                self.data.clone_ref(py),
                self.description.clone(),
            ),
        )
        .expect("Failed to create Messenger instance");

        messenger_instance.into_py(py)
    }
}

unsafe impl PyNativeType for PyInputParameter {}

pub fn register_session(session_id: Uuid) -> Arc<Mutex<Interpreter>> {
    let interpreter = Arc::new(Mutex::new(Interpreter::new()));
    INTERPRETERS.insert(session_id, interpreter.clone());
    interpreter
}

pub fn register_message_sender(session_id: Uuid, sender: mpsc::UnboundedSender<String>) {
    MESSAGE_SENDERS.insert(session_id, sender);
}

pub fn unregister_session(session_id: &Uuid) {
    INTERPRETERS.remove(session_id);
    MESSAGE_SENDERS.remove(session_id);
}

// Helper function to convert Python objects to JSON
fn py_to_json(py: Python, obj: &PyAny) -> Result<JsonValue, ServiceError> {
    if obj.is_none() {
        return Ok(JsonValue::Null);
    }

    if let Ok(val) = obj.extract::<bool>() {
        return Ok(JsonValue::Bool(val));
    }

    if let Ok(val) = obj.extract::<i64>() {
        return Ok(JsonValue::Number(val.into()));
    }

    if let Ok(val) = obj.extract::<f64>() {
        // Handle NaN and Infinity which aren't valid JSON
        if val.is_nan() {
            return Ok(JsonValue::String("NaN".to_string()));
        }
        if val.is_infinite() {
            let sign = if val.is_sign_positive() { "" } else { "-" };
            return Ok(JsonValue::String(format!("{}Infinity", sign)));
        }
        let value = serde_json::Number::from_f64(val)
            .unwrap_or_else(|| serde_json::Number::from_f64(0.0).unwrap());
        return Ok(JsonValue::Number(value));
    }

    if let Ok(val) = obj.extract::<String>() {
        return Ok(JsonValue::String(val));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let mut items = Vec::new();
        for item in list.iter() {
            items.push(py_to_json(py, item)?);
        }
        return Ok(JsonValue::Array(items));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            if let Ok(key_str) = key.extract::<String>() {
                map.insert(key_str, py_to_json(py, value)?);
            } else {
                // Non-string keys aren't valid in JSON, so convert to string
                let key_str = key.to_string();
                map.insert(key_str, py_to_json(py, value)?);
            }
        }
        return Ok(JsonValue::Object(map));
    }

    // For other types, convert to string representation
    Ok(JsonValue::String(obj.to_string()))
}

// Convert a JsonValue to a Python object
fn json_to_py(py: Python, value: &JsonValue) -> PyResult<PyObject> {
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(b) => Ok(b.to_object(py)),
        JsonValue::Number(n) => {
            if n.is_i64() {
                Ok(n.as_i64().unwrap().to_object(py))
            } else {
                Ok(n.as_f64().unwrap().to_object(py))
            }
        }
        JsonValue::String(s) => Ok(s.to_object(py)),
        JsonValue::Array(arr) => {
            let py_list = PyList::empty(py);
            for item in arr {
                py_list.append(json_to_py(py, item)?)?;
            }
            Ok(py_list.to_object(py))
        }
        JsonValue::Object(obj) => {
            let py_dict = PyDict::new(py);
            for (key, value) in obj {
                py_dict.set_item(key, json_to_py(py, value)?)?;
            }
            Ok(py_dict.to_object(py))
        }
    }
}

// Convert InputParameter to PyInputParameter
fn input_to_py_input(py: Python, input: &InputParamRequest) -> PyResult<PyInputParameter> {
    let py_data = json_to_py(py, &input.data)?;
    Ok(PyInputParameter::new(
        input.name.clone(),
        py_data,
        input.description.clone(),
    ))
}

pub async fn execute_script(
    session_id: &Uuid,
    code: &str,
    inputs: Option<Vec<InputParamRequest>>,
    interpreter: Arc<Mutex<Interpreter>>,
) -> Result<ExecutionResult, ServiceError> {
    // Acquire lock on the interpreter
    {
        let mut interpreter_lock = interpreter
            .try_lock()
            .expect("Failed to acquire lock on interpreter");
        interpreter_lock.reset_buffers();
    }

    let globals = {
        let interpreter_lock = interpreter
            .try_lock()
            .expect("Failed to acquire lock on interpreter");
        Arc::new(std::sync::Mutex::new(interpreter_lock.globals.clone()))
    };

    // Create stdout/stderr capture buffers
    let stdout_capture = {
        let interpreter_lock = interpreter
            .try_lock()
            .expect("Failed to acquire lock on interpreter");
        Arc::new(std::sync::Mutex::new(
            interpreter_lock.stdout_capture.clone(),
        ))
    };
    let stderr_capture = {
        let interpreter_lock = interpreter
            .try_lock()
            .expect("Failed to acquire lock on interpreter");
        Arc::new(std::sync::Mutex::new(
            interpreter_lock.stderr_capture.clone(),
        ))
    };

    let mut success = true;
    let mut result_value = None;
    let start_time = Instant::now();

    // Execute Python code with GIL
    Python::with_gil(|py| -> Result<(), ServiceError> {
        // Get the globals dictionary
        let mut binding = globals.try_lock().unwrap();
        let globals_guard = binding.as_ref();

        let globals = match globals_guard {
            Some(g) => g.clone_ref(py),
            None => {
                // If globals don't exist, create a new one
                let _globals = PyDict::new(py).to_object(py);
                let global_ref = _globals.clone_ref(py);
                *binding = Some(global_ref.clone_ref(py));
                global_ref
            }
        };

        let globals = globals.extract::<&PyDict>(py)?;

        package_manager::setup_venv_for_interpreter(py, globals, session_id)?;

        // Register Python classes
        if let Err(e) = py.run(
            r#"
class _InputParameterList:
    def __init__(self, params):
        self.params = params
    
    def __getitem__(self, index):
        return self.params[index]
    
    def __len__(self):
        return len(self.params)
    
    def __iter__(self):
        return iter(self.params)
        
    def get(self, name, default=None):
        for param in self.params:
            if param.name == name:
                return param
        return default
    
    def get_data(self, name, default=None):
        param = self.get(name)
        if param is not None:
            return param.data
        return default
    
    def __repr__(self):
        return f"InputParameterList({[p.name for p in self.params]})"
"#,
            Some(globals),
            None,
        ) {
            error!("Failed to define input parameter classes: {}", e);
            return Err(ServiceError::Python(format!(
                "Failed to define input parameter classes: {}",
                e
            )));
        }

        let in_list: Vec<&'_ PyAny> = Vec::new();
        let py_inputs_list = if let Some(inputs) = inputs {
            // Convert input parameters to Python objects
            let py_inputs: Vec<PyObject> = match inputs
                .iter()
                .map(|input| {
                    input_to_py_input(py, input)
                        .map(|py_input| py_input.into_py(py))
                        .map_err(|e| {
                            ServiceError::Python(format!(
                                "Failed to convert input parameter: {}",
                                e
                            ))
                        })
                })
                .collect()
            {
                Ok(inputs) => inputs,
                Err(e) => return Err(e),
            };

            // Create inputs list
            PyList::new(py, &py_inputs)
        } else {
            create_list(py, in_list)
        };

        // Create InputParameterList instance
        let input_param_class = py
            .eval("_InputParameterList", Some(globals), None)
            .map_err(|e| {
                ServiceError::Python(format!("Failed to get InputParameterList class: {}", e))
            })?;

        let args = PyTuple::new(py, &[py_inputs_list]);
        let inputs_obj = input_param_class.call1(args).map_err(|e| {
            ServiceError::Python(format!(
                "Failed to create InputParameterList instance: {}",
                e
            ))
        })?;
        // Add inputs to globals
        globals
            .set_item("_INPUTS", inputs_obj)
            .map_err(|e| ServiceError::Python(format!("Failed to set _INPUTS: {}", e)))?;

        // Set up the message sender function
        globals.set_item(
            "SessionMessenger",
            Messenger::new(session_id.to_string().as_str()),
        )?;

        // Setup stdout/stderr redirection using the io module
        let io = py.import("io")?;
        let sys = py.import("sys")?;

        // Create Python file-like objects that write to our Rust writers
        let stdout_io = io.call_method1("TextIOWrapper", (io.call_method1("BytesIO", ())?,))?;
        let stderr_io = io.call_method1("TextIOWrapper", (io.call_method1("BytesIO", ())?,))?;

        // Store original stdout/stderr
        let orig_stdout = sys.getattr("stdout")?;
        let orig_stderr = sys.getattr("stderr")?;

        // Replace stdout/stderr with our capturing versions
        sys.setattr("stdout", stdout_io)?;
        sys.setattr("stderr", stderr_io)?;

        // Import JSON for send_json helper
        let json = py.import("json")?;

        // Setup stdout/stderr redirection
        py.run(
            r#"
class Context:
    def get_inputs():
        """
        Get the input parameters passed to this script.

        Returns:
        An InputParameterList object containing all input parameters.
        You can access parameters by index, iterate through them,
        or look them up by name using the get() method.
        """
        return _INPUTS

    def send_output(data):
        """
        Send a JSON message back to the client
        
        Args:
            data: Any JSON-serializable Python object
        """

        import json
        
        try:
            message = data
            return SessionMessenger.send_message(message)
        except Exception as e:
            sys.stderr.write(f"Error sending message: {e}\n")
            return False
"#,
            Some(globals),
            None,
        )?;

        // First try to evaluate the code as an expression
        let result = match py.eval(code, Some(globals), None) {
            Ok(result) => {
                // Convert the result to JSON if it's not None
                if !result.is_none() {
                    match py_to_json(py, result) {
                        Ok(json_val) => {
                            result_value = Some(json_val);
                        }
                        Err(e) => {
                            // Log the error but don't fail
                            let err_str = format!("Error converting result to JSON: {}\n", e);
                            let mut stdout_lock = stdout_capture.try_lock();
                            let stdout_capture_lock = stdout_lock.as_mut().unwrap();
                            stdout_capture_lock.extend_from_slice(err_str.as_bytes());
                        }
                    }
                }
            }
            Err(err) => {
                // If evaluation failed, try running as a statement
                match py.run(code, Some(globals), None) {
                    Ok(_) => {
                        // Try to get a special return value variable
                        if let Ok(return_value) = globals.get_item("__return_value") {
                            if !return_value.is_none() {
                                match py_to_json(py, return_value.unwrap()) {
                                    Ok(json_val) => {
                                        result_value = Some(json_val);
                                    }
                                    Err(e) => {
                                        // Capture the error in stderr
                                        let err_str = format!("Error: {}\n", e);
                                        let mut std_err_lock = stderr_capture.try_lock();
                                        let stderr_capture_lock = std_err_lock.as_mut().unwrap();
                                        stderr_capture_lock.extend_from_slice(err_str.as_bytes());
                                        success = false;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Capture the error in stderr
                        let err_str = format!("Error: {}\n", e);
                        let mut std_err_lock = stderr_capture.try_lock();
                        let stderr_capture_lock = std_err_lock.as_mut().unwrap();
                        stderr_capture_lock.extend_from_slice(err_str.as_bytes());
                        success = false;
                    }
                }
            }
        };

        // Get the captured output
        if let Ok(flush_method) = stdout_io.getattr("flush") {
            let _ = flush_method.call0();
        }
        if let Ok(flush_method) = stderr_io.getattr("flush") {
            let _ = flush_method.call0();
        }

        // Get the contents from the in-memory buffer
        if let Ok(getvalue) = stdout_io
            .getattr("buffer")
            .and_then(|buf| buf.getattr("getvalue"))
        {
            if let Ok(bytes) = getvalue.call0() {
                if let Ok(bytes_data) = bytes.extract::<&[u8]>() {
                    stdout_capture
                        .try_lock()
                        .as_mut()
                        .unwrap()
                        .extend_from_slice(bytes_data);
                }
            }
        }

        if let Ok(getvalue) = stderr_io
            .getattr("buffer")
            .and_then(|buf| buf.getattr("getvalue"))
        {
            if let Ok(bytes) = getvalue.call0() {
                if let Ok(bytes_data) = bytes.extract::<&[u8]>() {
                    stderr_capture
                        .try_lock()
                        .as_mut()
                        .unwrap()
                        .extend_from_slice(bytes_data);
                }
            }
        }

        // Restore the original stdout/stderr
        sys.setattr("stdout", orig_stdout)?;
        sys.setattr("stderr", orig_stderr)?;

        Ok(result)
    })?;

    // Store the final globals back
    if let Some(g) = globals.try_lock().unwrap().as_ref() {
        Python::with_gil(|py| {
            interpreter.try_lock().unwrap().globals = Some(g.clone_ref(py));
        });
    }

    // Convert captured output to strings
    let stdout = {
        if let Ok(guard) = &stdout_capture.try_lock() {
            String::from_utf8_lossy(&guard).to_string()
        } else {
            String::new()
        }
    };

    let stderr = {
        if let Ok(guard) = &stderr_capture.try_lock() {
            String::from_utf8_lossy(&guard).to_string()
        } else {
            String::new()
        }
    };

    let execution_time = start_time.elapsed();

    Ok(ExecutionResult {
        stdout,
        stderr,
        result: result_value,
        success,
        execution_time,
        packages_installed: Vec::new(),
    })
}

// Execute Python script with timeout
pub async fn execute_script_with_timeout(
    session_id: &Uuid,
    code: &str,
    inputs: Option<Vec<InputParamRequest>>,
    requirements: Option<Vec<String>>,
    timeout_seconds: Option<u64>,
) -> Result<ExecutionResult, ServiceError> {
    let interpreter = match INTERPRETERS.get(session_id) {
        Some(i) => i.clone(),
        None => {
            return Err(ServiceError::Internal(
                "Interpreter session not found".to_string(),
            ));
        }
    };

    let session_id = session_id.clone();
    let code = code.to_string();
    let timeout_duration =
        Duration::from_secs(timeout_seconds.unwrap_or(DEFAULT_EXECUTION_TIMEOUT));

    // Create a channel for package installation progress updates
    let (progress_sender, _progress_receiver) = mpsc::unbounded_channel::<String>();
    let msg_sender_clone = MESSAGE_SENDERS.get(&session_id).map(|s| s.clone());

    if let Some(sender) = msg_sender_clone {
        // Forward package installation progress to the client
        let mut progress_receiver = _progress_receiver;
        tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let _ = sender.send(progress);
            }
        });
    }

    // Install required packages if specified
    let mut installed_packages = Vec::new();
    if let Some(reqs) = &requirements {
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

    let execution = tokio::task::spawn_blocking(move || {
        let result = futures::executor::block_on(execute_script(
            &session_id,
            &code,
            inputs,
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

    // Wait for the task to complete or timeout
    match tokio::time::timeout(timeout_duration, execution).await {
        Ok(result) => match result {
            Ok(inner_result) => Ok(inner_result?),
            Err(e) => Err(ServiceError::Internal(format!("Task panicked: {}", e))),
        },
        Err(_) => {
            // Timeout occurred
            Err(ServiceError::Internal(format!(
                "Script execution timed out after {} seconds",
                timeout_duration.as_secs()
            )))
        }
    }
}

pub fn create_number(py: Python<'_>, num: f64) -> &'_ PyFloat {
    PyFloat::new(py, num)
}

pub fn create_string(py: Python<'_>, val: String) -> &'_ PyString {
    PyString::new(py, &val)
}

pub fn create_bool(py: Python<'_>, val: bool) -> &'_ PyBool {
    PyBool::new(py, val)
}

pub fn create_list(py: Python<'_>, val: Vec<impl ToPyObject>) -> &'_ PyList {
    PyList::new(py, &val)
}

pub fn create_dict(py: Python<'_>) -> &'_ PyDict {
    PyDict::new(py)
}

pub fn dict_set_property(py: Python<'_>, dict: &'_ PyDict, key: &str, value: impl ToPyObject) {
    dict.set_item(key, value.to_object(py)).unwrap();
}

pub fn dict_get_property<'a>(py: Python<'a>, dict: &'a PyDict, key: &'a str) -> Option<&'a PyAny> {
    dict.get_item(key).expect("Failed to get property")
}

pub fn create_function<'a>(
    py: Python<'a>,
    name: String,
    func: impl Fn(&PyTuple, Option<&PyDict>) -> PyResult<&'a PyAny> + Send + 'static,
) -> PyResult<&'a PyCFunction> {
    let name: &'static str = Box::leak(Box::new(name));
    PyCFunction::new_closure(py, Some(&name), None, func)
}

pub fn dict_set_function<'a>(
    py: Python<'a>,
    dict: &'a PyDict,
    key: &'a str,
    func: &'a PyCFunction,
) {
    dict_set_property(py, dict, key, func)
}
