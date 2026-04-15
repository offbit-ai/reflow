//! Backend boundary for LiteRT-class inference in Reflow.
//!
//! V1 intentionally ships a deterministic mock backend. Real LiteRT Rust
//! bindings can be provided by a separate project and adapted behind these
//! traits without changing graph-facing actors.

use anyhow::{anyhow, bail, Result};
use reflow_media_types::{PacketMetadata, TensorDType, TensorPacket, TensorShape};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

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
}
