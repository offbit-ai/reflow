use deno_runtime::deno_core::error::AnyError;
use deno_runtime::deno_core::parking_lot::RwLock;
use deno_runtime::deno_core::{FsModuleLoader, ModuleCodeString, serde_v8, v8};
use deno_runtime::deno_fs::RealFs;
use deno_runtime::deno_napi::v8::{FunctionCallback, Local, MapFnTo};
use deno_runtime::deno_permissions::*;
use deno_runtime::permissions::RuntimePermissionDescriptorParser;
use deno_runtime::worker::{MainWorker, WorkerOptions, WorkerServiceOptions};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone)]
pub struct JavascriptRuntime {
    pub worker: Arc<RwLock<MainWorker>>,
    pub permissions: PermissionsContainer,
}

unsafe impl Send for JavascriptRuntime {}
unsafe impl Sync for JavascriptRuntime {}

impl JavascriptRuntime {
    pub fn new() -> Result<Self, AnyError> {
        let fs = RealFs;
        let blob_store = Arc::new(deno_runtime::deno_web::BlobStore::default());
        let stdio = deno_runtime::deno_io::Stdio::default();

        let permission_desc_parser = Arc::new(RuntimePermissionDescriptorParser::new(
            sys_traits::impls::RealSys,
        ));

        let permissions = PermissionsContainer::allow_all(permission_desc_parser);

        let worker = MainWorker::bootstrap_from_options::<
            deno_resolver::npm::DenoInNpmPackageChecker,
            deno_resolver::npm::NpmResolver<sys_traits::impls::RealSys>,
            sys_traits::impls::RealSys,
        >(
            &deno_runtime::deno_core::resolve_url("about:blank")?,
            WorkerServiceOptions {
                module_loader: Rc::new(FsModuleLoader),
                permissions: permissions.clone(),
                blob_store,
                broadcast_channel: Default::default(),
                feature_checker: Default::default(),
                node_services: Default::default(),
                npm_process_state_provider: Default::default(),
                root_cert_store_provider: Default::default(),
                fetch_dns_resolver: Default::default(),
                shared_array_buffer_store: Default::default(),
                compiled_wasm_module_store: Default::default(),
                v8_code_cache: Default::default(),
                fs: Arc::new(fs),
            },
            WorkerOptions {
                stdio,
                ..Default::default()
            },
        );

        let js_runtime = Self {
            worker: Arc::new(RwLock::new(worker)),
            permissions,
        };

        Ok(js_runtime)
    }

    pub async fn execute(
        &mut self,
        name: &str,
        source: &str,
    ) -> Result<v8::Global<v8::Value>, AnyError> {
        let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
        let source_static: &'static str = Box::leak(source.to_string().into_boxed_str());

        let result = {
            let mut runner = self.worker.write();
            let result =
                runner.execute_script(name_static, ModuleCodeString::from_static(source_static))?;
            result
        };

        // Check if the result is a promise and resolve it if needed
        let is_promise = {
            let mut runner = self.worker.write();
            let mut scope = runner.js_runtime.handle_scope();
            let local = v8::Local::new(&mut scope, result.clone());
            local.is_promise()
        };

        if is_promise {
            // Get the initial promise state
            let promise_state = {
                let mut runner = self.worker.write();
                let mut scope = runner.js_runtime.handle_scope();
                let local = v8::Local::new(&mut scope, result.clone());
                let promise = v8::Local::<v8::Promise>::try_from(local).unwrap();
                promise.state()
            };

            // Handle the promise based on its state
            match promise_state {
                v8::PromiseState::Pending => {
                    // Run the event loop until the promise is settled
                    let mut runner = self.worker.write();

                    runner.run_event_loop(false).await?;

                    // After running the event loop, check the promise state again
                    let mut scope = runner.js_runtime.handle_scope();
                    let local = v8::Local::new(&mut scope, result.clone());
                    let promise = v8::Local::<v8::Promise>::try_from(local).unwrap();

                    if promise.state() == v8::PromiseState::Fulfilled {
                        let res = promise.result(&mut scope);
                        return Ok(v8::Global::new(&mut scope, res));
                    } else if promise.state() == v8::PromiseState::Rejected {
                        let exception = promise.result(&mut scope);
                        return Err(AnyError::from(
                            deno_runtime::deno_core::error::JsError::from_v8_exception(
                                &mut scope, exception,
                            ),
                        ));
                    }
                }
                v8::PromiseState::Fulfilled => {
                    let mut runner = self.worker.write();
                    let mut scope = runner.js_runtime.handle_scope();
                    let local = v8::Local::new(&mut scope, result.clone());
                    let promise = v8::Local::<v8::Promise>::try_from(local).unwrap();
                    let res = promise.result(&mut scope);
                    return Ok(v8::Global::new(&mut scope, res));
                }
                v8::PromiseState::Rejected => {
                    let mut runner = self.worker.write();
                    let mut scope = runner.js_runtime.handle_scope();
                    let local = v8::Local::new(&mut scope, result.clone());
                    let promise = v8::Local::<v8::Promise>::try_from(local).unwrap();
                    let exception = promise.result(&mut scope);
                    return Err(AnyError::from(
                        deno_runtime::deno_core::error::JsError::from_v8_exception(
                            &mut scope, exception,
                        ),
                    ));
                }
            }
        }

        Ok(result)
    }

    // Convert a v8::Value to a Rust value
    pub fn convert_value_to_rust<T>(&self, value: v8::Global<v8::Value>) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let local = v8::Local::new(&mut scope, value);
        let json =
            serde_v8::from_v8(&mut scope, local).expect("Could not serialize javascript value");
        serde_json::from_value(json).expect("Could not convert value")
    }

    pub fn convert_value_from_rust(
        &self,
        value: serde_json::Value,
    ) -> Result<v8::Global<v8::Value>, AnyError> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let val = serde_v8::to_v8(&mut scope, value)?;
        Ok(v8::Global::new(&mut scope, val))
    }

    // Register a Rust function as a global variable in the JavaScript runtime
    pub fn register_function(&mut self, name: &str, func: impl MapFnTo<FunctionCallback>) {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let func = v8::FunctionTemplate::new(&mut scope, func);
        let func = func.get_function(&mut scope).unwrap();
        let name = v8::String::new(&mut scope, name).unwrap().into();
        scope
            .get_current_context()
            .global(&mut scope)
            .set(&mut scope, name, func.into());
    }

    // Call a function in the JavaScript runtime
    pub fn call_function(
        &mut self,
        name: &str,
        args: &[v8::Global<v8::Value>],
    ) -> Result<v8::Global<v8::Value>, AnyError> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let name = v8::String::new(&mut scope, name).unwrap().into();
        let global = scope.get_current_context().global(&mut scope);
        let func = global
            .get(&mut scope, name)
            .expect("Failed to find function");
        let func = v8::Local::<v8::Function>::try_from(func)?;
        // extract args into Local
        let mut _args = Vec::new();
        for arg in args {
            let arg = v8::Local::new(&mut scope, arg);
            _args.push(arg);
        }
        let res = func
            .call(&mut scope, global.into(), &_args)
            .expect("Failed to call function");
        Ok(v8::Global::new(&mut scope, res))
    }

    pub fn create_number(&mut self, number: f64) -> v8::Global<v8::Value> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _number: Local<v8::Value> = v8::Number::new(&mut scope, number).into();
        v8::Global::new(&mut scope, _number)
    }

    pub fn create_string(&mut self, string: &str) -> v8::Global<v8::Value> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _string: Local<v8::Value> = v8::String::new(&mut scope, string).unwrap().into();
        v8::Global::new(&mut scope, _string)
    }

    pub fn create_array(&mut self, array: &[v8::Global<v8::Value>]) -> v8::Global<v8::Value> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _array = v8::Array::new(&mut scope, array.len() as i32);

        for (i, item) in array.iter().enumerate() {
            let item = v8::Local::new(&mut scope, item);
            let _i = v8::Number::new(&mut scope, i as f64);
            let key = v8::Local::new(&mut scope, _i);
            _array.set(&mut scope, key.into(), item).unwrap();
        }
        let local_array: Local<v8::Value> = v8::Local::new(&mut scope, _array).into();
        v8::Global::new(&mut scope, local_array)
    }

    pub fn create_object(&mut self) -> v8::Global<v8::Object> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _obj = v8::Object::new(&mut scope);
        v8::Global::new(&mut scope, _obj)
    }
    pub fn object_set_property(
        &mut self,
        object: v8::Global<v8::Object>,
        key: &str,
        value: v8::Global<v8::Value>,
    ) -> v8::Global<v8::Object> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _obj = v8::Local::new(&mut scope, object);
        let _key: v8::Local<v8::Value> = v8::String::new(&mut scope, key).unwrap().into();
        let _value = v8::Local::new(&mut scope, value);
        _obj.set(&mut scope, _key, _value).unwrap();
        v8::Global::new(&mut scope, _obj)
    }
    pub fn obj_to_value(&mut self, obj: v8::Global<v8::Object>) -> v8::Global<v8::Value> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let _obj: v8::Local<v8::Value> = v8::Local::new(&mut scope, obj).into();
        v8::Global::new(&mut scope, _obj)
    }

    pub fn declare_function(
        &mut self,
        func: impl MapFnTo<FunctionCallback>,
    ) -> v8::Global<v8::Value> {
        let mut runner = self.worker.write();
        let mut scope = runner.js_runtime.handle_scope();
        let func = v8::FunctionTemplate::new(&mut scope, func);
        let func: v8::Local<v8::Value> = func.get_function(&mut scope).unwrap().into();
        v8::Global::new(&mut scope, func)
    }
}

#[cfg(test)]
mod tests;
