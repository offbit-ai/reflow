//! Backend boundary for LiteRT-class inference in Reflow.
//!
//! The default build remains deterministic and mock-first so CI and graph
//! authoring do not require native ML libraries. The real LiteRT adapter is
//! optional and targets the published Offbit `litert` crates, wiring them in
//! behind the same backend traits without changing graph-facing actors or
//! taskpacks.

use anyhow::{anyhow, bail, Result};
use reflow_media_types::{PacketMetadata, TensorDType, TensorPacket, TensorShape};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "external-litert")]
pub use external_litert::LiteRtBackend;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorSpec {
    pub name: String,
    pub dtype: TensorDType,
    pub shape: TensorShape,
}

impl TensorSpec {
    pub fn byte_len(&self) -> usize {
        self.shape.element_count() * self.dtype.bytes_per_element()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub backend: String,
    pub task: String,
    #[serde(default)]
    pub inputs: Vec<TensorSpec>,
    #[serde(default)]
    pub outputs: Vec<TensorSpec>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, Value>,
}

impl ModelInfo {
    pub fn mock(id: impl Into<String>, task: impl Into<String>, outputs: Vec<TensorSpec>) -> Self {
        Self {
            id: id.into(),
            backend: "mock".to_string(),
            task: task.into(),
            inputs: Vec::new(),
            outputs,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceInput {
    pub name: String,
    pub tensor: TensorPacket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceOutput {
    #[serde(default)]
    pub tensors: Vec<TensorPacket>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, Value>,
}

pub trait InferenceBackend: Send + Sync {
    fn name(&self) -> &str;

    fn load_model(
        &self,
        model: ModelInfo,
        model_data: Option<Arc<Vec<u8>>>,
    ) -> Result<Box<dyn InferenceSession>>;
}

pub trait InferenceSession: Send + Sync {
    fn model_info(&self) -> &ModelInfo;

    fn run(&self, inputs: &[InferenceInput]) -> Result<InferenceOutput>;
}

#[derive(Debug, Clone, Default)]
pub struct MockBackend;

impl MockBackend {
    pub fn new() -> Self {
        Self
    }
}

impl InferenceBackend for MockBackend {
    fn name(&self) -> &str {
        "mock"
    }

    fn load_model(
        &self,
        model: ModelInfo,
        model_data: Option<Arc<Vec<u8>>>,
    ) -> Result<Box<dyn InferenceSession>> {
        if model.backend != "mock" && model.backend != "litert" {
            bail!("mock backend cannot load backend '{}'", model.backend);
        }
        Ok(Box::new(MockSession {
            model,
            model_data_len: model_data.as_ref().map(|bytes| bytes.len()).unwrap_or(0),
        }))
    }
}

#[derive(Debug, Clone)]
struct MockSession {
    model: ModelInfo,
    model_data_len: usize,
}

impl InferenceSession for MockSession {
    fn model_info(&self) -> &ModelInfo {
        &self.model
    }

    fn run(&self, inputs: &[InferenceInput]) -> Result<InferenceOutput> {
        let outputs = if self.model.outputs.is_empty() {
            infer_default_outputs(inputs)?
        } else {
            self.model
                .outputs
                .iter()
                .map(|spec| deterministic_tensor(spec, &self.model, inputs))
                .collect()
        };

        Ok(InferenceOutput {
            tensors: outputs,
            metadata: HashMap::from([
                ("backend".to_string(), json!("mock")),
                ("modelId".to_string(), json!(self.model.id)),
                ("modelBytes".to_string(), json!(self.model_data_len)),
                ("inputCount".to_string(), json!(inputs.len())),
            ]),
        })
    }
}

#[cfg(feature = "external-litert")]
mod external_litert {
    use super::*;
    use litert::{
        Accelerators, CompilationOptions, CompiledModel, ElementType, Environment, Model,
        TensorBuffer,
    };
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    pub struct LiteRtBackend {
        accelerators: Accelerators,
    }

    impl LiteRtBackend {
        pub fn new() -> Self {
            Self {
                accelerators: Accelerators::CPU,
            }
        }

        pub fn with_accelerators(accelerators: Accelerators) -> Self {
            Self { accelerators }
        }
    }

    impl Default for LiteRtBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl InferenceBackend for LiteRtBackend {
        fn name(&self) -> &str {
            "litert"
        }

        fn load_model(
            &self,
            model: ModelInfo,
            model_data: Option<Arc<Vec<u8>>>,
        ) -> Result<Box<dyn InferenceSession>> {
            if model.backend != "litert" {
                bail!("LiteRT backend cannot load backend '{}'", model.backend);
            }
            let model_data =
                model_data.ok_or_else(|| anyhow!("LiteRT backend requires model_data bytes"))?;
            let env = Environment::new().map_err(|err| anyhow!("LiteRT environment: {err}"))?;
            let litert_model = Model::from_bytes(model_data.as_ref().clone().into_boxed_slice())
                .map_err(|err| anyhow!("LiteRT model load: {err}"))?;
            let signature = litert_model
                .signature(0)
                .map_err(|err| anyhow!("LiteRT signature 0: {err}"))?;
            let input_shapes = (0..signature.input_count()?)
                .map(|index| signature.input_shape(index))
                .collect::<litert::Result<Vec<_>>>()
                .map_err(|err| anyhow!("LiteRT input shape introspection: {err}"))?;
            let output_shapes = (0..signature.output_count()?)
                .map(|index| signature.output_shape(index))
                .collect::<litert::Result<Vec<_>>>()
                .map_err(|err| anyhow!("LiteRT output shape introspection: {err}"))?;
            validate_specs("input", &model.inputs, &input_shapes)?;
            validate_specs("output", &model.outputs, &output_shapes)?;
            let accelerators = accelerators_from_model(&model, self.accelerators)?;
            let options = CompilationOptions::new()
                .and_then(|options| options.with_accelerators(accelerators))
                .map_err(|err| anyhow!("LiteRT compilation options: {err}"))?;
            let mut input_buffers = input_shapes
                .iter()
                .map(|shape| TensorBuffer::managed_host(&env, shape))
                .collect::<litert::Result<Vec<_>>>()
                .map_err(|err| anyhow!("LiteRT input buffer allocation: {err}"))?;
            let mut output_buffers = output_shapes
                .iter()
                .map(|shape| TensorBuffer::managed_host(&env, shape))
                .collect::<litert::Result<Vec<_>>>()
                .map_err(|err| anyhow!("LiteRT output buffer allocation: {err}"))?;
            let compiled = CompiledModel::new(env, litert_model, &options)
                .map_err(|err| anyhow!("LiteRT compile: {err}"))?;
            let fully_accelerated = compiled
                .is_fully_accelerated()
                .map_err(|err| anyhow!("LiteRT acceleration query: {err}"))?;

            // Keep buffers allocated against the same environment that owns the compiled model.
            input_buffers.shrink_to_fit();
            output_buffers.shrink_to_fit();

            Ok(Box::new(LiteRtSession {
                model,
                state: Mutex::new(LiteRtSessionState {
                    compiled,
                    input_buffers,
                    output_buffers,
                    input_shapes,
                    output_shapes,
                    accelerators,
                    fully_accelerated,
                }),
            }))
        }
    }

    struct LiteRtSession {
        model: ModelInfo,
        state: Mutex<LiteRtSessionState>,
    }

    struct LiteRtSessionState {
        compiled: CompiledModel,
        input_buffers: Vec<TensorBuffer>,
        output_buffers: Vec<TensorBuffer>,
        input_shapes: Vec<litert::TensorShape>,
        output_shapes: Vec<litert::TensorShape>,
        accelerators: Accelerators,
        fully_accelerated: bool,
    }

    impl InferenceSession for LiteRtSession {
        fn model_info(&self) -> &ModelInfo {
            &self.model
        }

        fn run(&self, inputs: &[InferenceInput]) -> Result<InferenceOutput> {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("LiteRT session state was poisoned"))?;
            if inputs.len() != state.input_shapes.len() {
                bail!(
                    "LiteRT input count mismatch: model expects {}, graph provided {}",
                    state.input_shapes.len(),
                    inputs.len()
                );
            }

            for index in 0..state.input_buffers.len() {
                let input = input_for_index(&self.model, inputs, index)?;
                let shape = state.input_shapes[index].clone();
                let buffer = &mut state.input_buffers[index];
                write_tensor_to_buffer(&input.tensor, buffer, &shape)?;
            }

            {
                let LiteRtSessionState {
                    compiled,
                    input_buffers,
                    output_buffers,
                    ..
                } = &mut *state;
                compiled
                    .run(input_buffers, output_buffers)
                    .map_err(|err| anyhow!("LiteRT inference: {err}"))?;
            }

            let mut tensors = Vec::with_capacity(state.output_buffers.len());
            for index in 0..state.output_buffers.len() {
                let buffer = &state.output_buffers[index];
                let shape = state.output_shapes[index].clone();
                let name = self
                    .model
                    .outputs
                    .get(index)
                    .map(|spec| spec.name.clone())
                    .unwrap_or_else(|| format!("output_{index}"));
                tensors.push(read_tensor_from_buffer(name, buffer, &shape)?);
            }
            let fully_accelerated = state.fully_accelerated;
            let accelerators = state.accelerators;

            Ok(InferenceOutput {
                tensors,
                metadata: HashMap::from([
                    ("backend".to_string(), json!("litert")),
                    ("modelId".to_string(), json!(self.model.id)),
                    ("inputCount".to_string(), json!(inputs.len())),
                    (
                        "accelerators".to_string(),
                        json!(accelerator_names(accelerators)),
                    ),
                    ("fullyAccelerated".to_string(), json!(fully_accelerated)),
                ]),
            })
        }
    }

    fn input_for_index<'a>(
        model: &ModelInfo,
        inputs: &'a [InferenceInput],
        index: usize,
    ) -> Result<&'a InferenceInput> {
        if let Some(spec) = model.inputs.get(index) {
            if let Some(input) = inputs.iter().find(|input| {
                input.name == spec.name || input.tensor.name.as_deref() == Some(spec.name.as_str())
            }) {
                return Ok(input);
            }
        }
        inputs
            .get(index)
            .ok_or_else(|| anyhow!("missing LiteRT input tensor at index {index}"))
    }

    fn write_tensor_to_buffer(
        tensor: &TensorPacket,
        buffer: &mut TensorBuffer,
        shape: &litert::TensorShape,
    ) -> Result<()> {
        let expected_dtype = dtype_from_element_type(shape.element_type)?;
        if tensor.dtype != expected_dtype {
            bail!(
                "LiteRT tensor dtype mismatch for {:?}: expected {:?}, got {:?}",
                tensor.name,
                expected_dtype,
                tensor.dtype
            );
        }
        let expected_dims = dims_from_litert(shape)?;
        if tensor.shape.dims != expected_dims {
            bail!(
                "LiteRT tensor shape mismatch for {:?}: expected {:?}, got {:?}",
                tensor.name,
                expected_dims,
                tensor.shape.dims
            );
        }
        validate_tensor_byte_len(tensor)?;

        match shape.element_type {
            ElementType::Float32 => copy_to_buffer::<f32>(buffer, &read_f32_values(tensor)?)?,
            ElementType::UInt8 => copy_to_buffer::<u8>(buffer, &tensor.data)?,
            ElementType::Int8 => copy_to_buffer::<i8>(buffer, &read_i8_values(tensor)?)?,
            ElementType::Int32 => copy_to_buffer::<i32>(buffer, &read_i32_values(tensor)?)?,
            ElementType::Int64 => copy_to_buffer::<i64>(buffer, &read_i64_values(tensor)?)?,
            ElementType::Bool => copy_to_buffer::<bool>(buffer, &read_bool_values(tensor)?)?,
            other => bail!("LiteRT input element type {:?} is not supported yet", other),
        }
        Ok(())
    }

    fn read_tensor_from_buffer(
        name: String,
        buffer: &TensorBuffer,
        shape: &litert::TensorShape,
    ) -> Result<TensorPacket> {
        let dtype = dtype_from_element_type(shape.element_type)?;
        let dims = TensorShape::new(dims_from_litert(shape)?);
        let data = match shape.element_type {
            ElementType::Float32 => {
                let values = buffer.lock_for_read::<f32>()?;
                f32_values_to_bytes(&values)
            }
            ElementType::UInt8 => buffer.lock_for_read::<u8>()?.to_vec(),
            ElementType::Int8 => buffer
                .lock_for_read::<i8>()?
                .iter()
                .map(|value| *value as u8)
                .collect(),
            ElementType::Int32 => buffer
                .lock_for_read::<i32>()?
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect(),
            ElementType::Int64 => buffer
                .lock_for_read::<i64>()?
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect(),
            ElementType::Bool => buffer
                .lock_for_read::<bool>()?
                .iter()
                .map(|value| u8::from(*value))
                .collect(),
            other => bail!(
                "LiteRT output element type {:?} is not supported yet",
                other
            ),
        };

        Ok(TensorPacket::new(Some(name), dtype, dims, data))
    }

    fn copy_to_buffer<T: litert::TensorElement>(
        buffer: &mut TensorBuffer,
        values: &[T],
    ) -> Result<()> {
        let mut guard = buffer.lock_for_write::<T>()?;
        if guard.len() != values.len() {
            bail!(
                "LiteRT buffer element count mismatch: expected {}, got {}",
                guard.len(),
                values.len()
            );
        }
        guard.copy_from_slice(values);
        Ok(())
    }

    fn validate_tensor_byte_len(tensor: &TensorPacket) -> Result<()> {
        let expected = tensor.expected_byte_len();
        if tensor.data.len() != expected {
            bail!(
                "tensor {:?} byte length mismatch: expected {}, got {}",
                tensor.name,
                expected,
                tensor.data.len()
            );
        }
        Ok(())
    }

    fn validate_specs(
        label: &str,
        specs: &[TensorSpec],
        shapes: &[litert::TensorShape],
    ) -> Result<()> {
        if specs.is_empty() {
            return Ok(());
        }
        if specs.len() != shapes.len() {
            bail!(
                "LiteRT {label} spec count mismatch: manifest declares {}, model exposes {}",
                specs.len(),
                shapes.len()
            );
        }
        for (index, (spec, shape)) in specs.iter().zip(shapes.iter()).enumerate() {
            let dtype = dtype_from_element_type(shape.element_type)?;
            let dims = dims_from_litert(shape)?;
            if spec.dtype != dtype || spec.shape.dims != dims {
                bail!(
                    "LiteRT {label} spec mismatch at index {index} ({:?}): manifest {:?} {:?}, model {:?} {:?}",
                    spec.name,
                    spec.dtype,
                    spec.shape.dims,
                    dtype,
                    dims
                );
            }
        }
        Ok(())
    }

    fn dims_from_litert(shape: &litert::TensorShape) -> Result<Vec<usize>> {
        shape
            .dims
            .iter()
            .map(|dim| {
                usize::try_from(*dim)
                    .map_err(|_| anyhow!("LiteRT tensor has negative dimension {dim}"))
            })
            .collect()
    }

    fn dtype_from_element_type(element_type: ElementType) -> Result<TensorDType> {
        Ok(match element_type {
            ElementType::Float32 => TensorDType::F32,
            ElementType::Int32 => TensorDType::I32,
            ElementType::Int64 => TensorDType::I64,
            ElementType::UInt8 => TensorDType::U8,
            ElementType::Int8 => TensorDType::I8,
            ElementType::Bool => TensorDType::Bool,
            other => bail!("LiteRT element type {:?} is not supported yet", other),
        })
    }

    fn accelerators_from_model(model: &ModelInfo, default: Accelerators) -> Result<Accelerators> {
        let Some(value) = model
            .metadata
            .get("accelerators")
            .or_else(|| model.metadata.get("accelerator"))
        else {
            return Ok(default);
        };

        match value {
            Value::String(text) => parse_accelerator_list(text),
            Value::Array(values) => {
                let mut accelerators = Accelerators::NONE;
                for value in values {
                    let Some(name) = value.as_str() else {
                        bail!("accelerators metadata array must contain strings");
                    };
                    accelerators = accelerators | parse_accelerator(name)?;
                }
                Ok(accelerators)
            }
            _ => bail!("accelerators metadata must be a string or array of strings"),
        }
    }

    fn parse_accelerator_list(text: &str) -> Result<Accelerators> {
        let mut accelerators = Accelerators::NONE;
        for part in text.split([',', '|', '+']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            accelerators = accelerators | parse_accelerator(part)?;
        }
        Ok(accelerators)
    }

    fn parse_accelerator(name: &str) -> Result<Accelerators> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Accelerators::NONE),
            "cpu" => Ok(Accelerators::CPU),
            "gpu" | "metal" => Ok(Accelerators::GPU),
            "npu" => Ok(Accelerators::NPU),
            other => bail!("unsupported LiteRT accelerator '{other}'"),
        }
    }

    fn accelerator_names(accelerators: Accelerators) -> Vec<&'static str> {
        if accelerators == Accelerators::NONE {
            return vec!["none"];
        }
        let mut names = Vec::new();
        if accelerators.contains(Accelerators::CPU) {
            names.push("cpu");
        }
        if accelerators.contains(Accelerators::GPU) {
            names.push("gpu");
        }
        if accelerators.contains(Accelerators::NPU) {
            names.push("npu");
        }
        names
    }

    fn read_f32_values(tensor: &TensorPacket) -> Result<Vec<f32>> {
        tensor
            .as_f32_vec()
            .ok_or_else(|| anyhow!("expected f32 tensor bytes"))
    }

    fn read_i8_values(tensor: &TensorPacket) -> Result<Vec<i8>> {
        Ok(tensor.data.iter().map(|value| *value as i8).collect())
    }

    fn read_i32_values(tensor: &TensorPacket) -> Result<Vec<i32>> {
        read_chunks::<4, i32>(&tensor.data, i32::from_le_bytes)
    }

    fn read_i64_values(tensor: &TensorPacket) -> Result<Vec<i64>> {
        read_chunks::<8, i64>(&tensor.data, i64::from_le_bytes)
    }

    fn read_bool_values(tensor: &TensorPacket) -> Result<Vec<bool>> {
        Ok(tensor.data.iter().map(|value| *value != 0).collect())
    }

    fn read_chunks<const N: usize, T>(
        data: &[u8],
        decode: impl Fn([u8; N]) -> T,
    ) -> Result<Vec<T>> {
        if data.len() % N != 0 {
            bail!("tensor byte length {} is not divisible by {N}", data.len());
        }
        Ok(data
            .chunks_exact(N)
            .map(|chunk| {
                let mut bytes = [0u8; N];
                bytes.copy_from_slice(chunk);
                decode(bytes)
            })
            .collect())
    }

    fn f32_values_to_bytes(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }
}

fn infer_default_outputs(inputs: &[InferenceInput]) -> Result<Vec<TensorPacket>> {
    let first = inputs
        .first()
        .ok_or_else(|| anyhow!("mock inference requires at least one input tensor"))?;
    let spec = TensorSpec {
        name: "output".to_string(),
        dtype: TensorDType::F32,
        shape: TensorShape::new([1, first.tensor.shape.element_count().clamp(1, 16)]),
    };
    Ok(vec![deterministic_tensor(
        &spec,
        &ModelInfo::mock("mock", "generic", vec![spec.clone()]),
        inputs,
    )])
}

fn deterministic_tensor(
    spec: &TensorSpec,
    model: &ModelInfo,
    inputs: &[InferenceInput],
) -> TensorPacket {
    let count = spec.shape.element_count();
    let seed = stable_seed(model, inputs, &spec.name);
    let mut metadata = PacketMetadata::default();
    if let Some(first) = inputs.first() {
        metadata.merge_missing_from(&first.tensor.metadata);
    }
    metadata.fields.insert("mockSeed".to_string(), json!(seed));

    match spec.dtype {
        TensorDType::F32 => {
            let mut values = Vec::with_capacity(count);
            for i in 0..count {
                let raw = seed.wrapping_add((i as u64).wrapping_mul(1_103_515_245));
                values.push(((raw % 10_000) as f32 / 10_000.0).clamp(0.0, 1.0));
            }
            let mut tensor =
                TensorPacket::from_f32(Some(spec.name.clone()), spec.shape.clone(), &values);
            tensor.metadata = metadata;
            tensor
        }
        TensorDType::U8 => {
            let data = (0..count)
                .map(|i| seed.wrapping_add(i as u64) as u8)
                .collect::<Vec<_>>();
            let mut tensor = TensorPacket::new(
                Some(spec.name.clone()),
                TensorDType::U8,
                spec.shape.clone(),
                data,
            );
            tensor.metadata = metadata;
            tensor
        }
        _ => {
            let bytes = vec![0u8; spec.byte_len()];
            let mut tensor = TensorPacket::new(
                Some(spec.name.clone()),
                spec.dtype,
                spec.shape.clone(),
                bytes,
            );
            tensor.metadata = metadata;
            tensor
        }
    }
}

fn stable_seed(model: &ModelInfo, inputs: &[InferenceInput], output_name: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;
    for byte in model.id.bytes().chain(output_name.bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    for input in inputs {
        for byte in input.name.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(1_099_511_628_211);
        }
        hash ^= input.tensor.data.len() as u64;
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_backend_is_deterministic() {
        let model = ModelInfo::mock(
            "hand-landmark",
            "landmark",
            vec![TensorSpec {
                name: "landmarks".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 6]),
            }],
        );
        let input = InferenceInput {
            name: "image".to_string(),
            tensor: TensorPacket::from_f32(
                Some("image".to_string()),
                TensorShape::new([1, 2]),
                &[0.0, 1.0],
            ),
        };
        let backend = MockBackend::new();
        let session = backend.load_model(model, None).unwrap();

        let a = session.run(std::slice::from_ref(&input)).unwrap();
        let b = session.run(&[input]).unwrap();

        assert_eq!(a, b);
        assert_eq!(a.tensors[0].shape.dims, vec![1, 6]);
    }

    #[cfg(feature = "external-litert")]
    #[test]
    fn external_litert_backend_runs_bundled_add_fixture() -> Result<()> {
        let _ = litert::set_global_log_severity(litert::LogSeverity::Error);
        let model_data = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/data/add_10x10.tflite"),
        )?;
        let shape = TensorShape::new([10, 10]);
        let model = ModelInfo {
            id: "add_10x10".to_string(),
            backend: "litert".to_string(),
            task: "elementwise_add".to_string(),
            inputs: vec![
                TensorSpec {
                    name: "lhs".to_string(),
                    dtype: TensorDType::F32,
                    shape: shape.clone(),
                },
                TensorSpec {
                    name: "rhs".to_string(),
                    dtype: TensorDType::F32,
                    shape: shape.clone(),
                },
            ],
            outputs: vec![TensorSpec {
                name: "sum".to_string(),
                dtype: TensorDType::F32,
                shape: shape.clone(),
            }],
            metadata: HashMap::new(),
        };
        let lhs = (0..100).map(|index| index as f32).collect::<Vec<_>>();
        let rhs = (0..100)
            .map(|index| 100.0 + index as f32)
            .collect::<Vec<_>>();
        let backend = LiteRtBackend::new();
        let session = backend.load_model(model, Some(Arc::new(model_data)))?;
        let output = session.run(&[
            InferenceInput {
                name: "lhs".to_string(),
                tensor: TensorPacket::from_f32(Some("lhs".to_string()), shape.clone(), &lhs),
            },
            InferenceInput {
                name: "rhs".to_string(),
                tensor: TensorPacket::from_f32(Some("rhs".to_string()), shape, &rhs),
            },
        ])?;

        let values = output.tensors[0].as_f32_vec().unwrap();
        assert_eq!(values.len(), 100);
        for (index, value) in values.iter().enumerate() {
            assert!(
                (*value - (100.0 + 2.0 * index as f32)).abs() < 1e-6,
                "element {index}: got {value}"
            );
        }
        assert_eq!(output.metadata.get("backend"), Some(&json!("litert")));
        Ok(())
    }
}
