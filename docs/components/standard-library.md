# Standard Component Library

Reflow's standard library provides native actor implementations for Zeal workflow templates. These are Rust actors compiled into the `reflow_components` crate.

Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB via `ComponentSpec::Script` — this crate only contains native actors.

## Overview

The component library is organized into these modules:

| Module | Actors | Template IDs |
|--------|--------|-------------|
| **Flow Control** | `ConditionalBranchActor`, `SwitchCaseActor`, `LoopActor` | `tpl_if_branch`, `tpl_switch`, `tpl_loop` |
| **Transform** | `DataTransformActor`, `DataOperationsActor` | `tpl_data_transformer`, `tpl_data_operations` |
| **Integration** | `HttpRequestActor` | `tpl_http_request` |
| **Logic** | `RulesEngineActor` | `tpl_rules_engine` |
| **Media** | `ImageInputActor`, `AudioInputActor`, `VideoInputActor`, `CameraCaptureActor` | `tpl_image_input`, `tpl_audio_input`, `tpl_video_input`, `tpl_camera_capture` |
| **Media / ML** (feature-gated) | CV preprocess, inference, decode, taskpack actors | `tpl_ml_*`, `tpl_cv_*` |
| **API** (feature-gated) | 6,697 generated actors | `api_*` (88 services) |

## Template Registry

All actors are resolved via `get_actor_for_template(template_id)` in `registry.rs`. The registry maps Zeal template IDs to actor instances:

```rust
use reflow_components::get_actor_for_template;

// Returns Some(Arc<dyn Actor>) for known templates
let actor = get_actor_for_template("tpl_http_request");

// Falls through to API actors when the `api` feature is enabled
let api_actor = get_actor_for_template("api_slack_send_message");
```

The complete template mapping is available via `get_template_mapping()` which returns a `HashMap<String, String>` of template ID to actor name.

## Flow Control

### ConditionalBranchActor (`tpl_if_branch`)

Routes messages based on conditions. Evaluates an input condition and sends data to either the `true` or `false` output port.

**Ports:**
- Input: `In` (data), `Condition` (boolean expression)
- Output: `True`, `False`

### SwitchCaseActor (`tpl_switch`)

Multi-way routing based on matching a value against multiple cases. Similar to a switch/case statement.

**Ports:**
- Input: `In` (data), `Match` (value to match)
- Output: Dynamic output ports per case, plus `Default`

### LoopActor (`tpl_loop`)

Iterative processing. Sends each item from an array to the loop body and collects results.

**Ports:**
- Input: `In` (array of items)
- Output: `Item` (current iteration item), `Done` (collected results)

## Transform

### DataTransformActor (`tpl_data_transformer`)

Applies transformations to input data using configurable operations like mapping, filtering, and restructuring.

**Ports:**
- Input: `In`
- Output: `Out`, `Error`

### DataOperationsActor (`tpl_data_operations`)

Performs data manipulation operations (sort, filter, group, aggregate) with support for inline JavaScript expressions via `rquickjs` for complex logic.

The JS evaluation is lightweight and embedded — it creates a `rquickjs::Runtime` with `input` and `data` globals bound, not a full script execution environment.

**Ports:**
- Input: `In`
- Output: `Out`, `Error`

## Integration

### HttpRequestActor (`tpl_http_request`)

Makes HTTP requests to external APIs and services.

**Ports:**
- Input: `Url`, `Method`, `Headers`, `Body`
- Output: `Response`, `Error`

## Logic

### RulesEngineActor (`tpl_rules_engine`)

Evaluates business rules against input data. Rules are defined as conditions with associated actions.

**Ports:**
- Input: `In`, `Rules`
- Output: `Out`, `Error`

## Media

### ImageInputActor (`tpl_image_input`)

Handles image input with metadata extraction (dimensions, format, EXIF data).

**Ports:**
- Input: `In` (image data or URL)
- Output: `Out` (image with metadata), `Error`

### AudioInputActor (`tpl_audio_input`)

Handles audio input with metadata extraction (duration, format, sample rate).

**Ports:**
- Input: `In` (audio data or URL)
- Output: `Out` (audio with metadata), `Error`

### VideoInputActor (`tpl_video_input`)

Handles video input with metadata extraction (duration, resolution, codec).

**Ports:**
- Input: `In` (video data or URL)
- Output: `Out` (video with metadata), `Error`

### CameraCaptureActor (`tpl_camera_capture`)

Produces `video/raw-rgba` streams from a deterministic mock camera by default. Native device capture is available with the `camera-native` feature and `backend: "native"`.

**Ports:**
- Input: `start`, `stop`
- Output: `stream`, `metadata`, `error`

## Media / ML Stack

The optional `ml` feature registers graph-driven media and ML actors for tensor packets, CV preprocessing, model loading, inference, detection/landmark decoding, temporal smoothing, and taskpack subgraphs.

The stack is mock-first by default. Real LiteRT execution is available through a separate `external-litert` feature on the ML crates, behind Reflow's inference backend traits.

See [Media / ML Stack](./ml-stack.md) for the architecture, feature flags, and verification commands.

## API Service Actors

When the `api` Cargo feature is enabled (default), 6,697 pre-generated actors for 88 API services are available. These are code-generated from OpenAPI specifications.

See [API Service Actors](./api-actors.md) for the full list.

## Feature Flags

The `api` feature controls compilation of the generated API modules. The `ml` feature enables the optional Media / ML actor catalog.

```toml
# Cargo.toml
[dependencies]
reflow_components = { path = "../reflow_components" }  # api enabled by default

# For faster test builds, disable api:
reflow_components = { path = "../reflow_components", default-features = false }

# For Media / ML templates:
reflow_components = { path = "../reflow_components", features = ["ml"] }

# For native camera capture:
reflow_components = { path = "../reflow_components", features = ["camera-native"] }
```

When `api` is disabled, stub types are provided so dependents compile without the heavy API modules:

```rust
// Stubs when api feature is disabled
pub fn get_api_template_infos() -> &'static [ApiTemplateInfo] { &[] }
pub fn get_api_actor_for_template(_: &str) -> Option<Arc<dyn Actor>> { None }
```

## Next Steps

- [API Service Actors](./api-actors.md) - 6,697 generated actors across 88 services
- [Media Actors](./media-actors.md) - Image, audio, and video processing
- [Media / ML Stack](./ml-stack.md) - Tensor, CV, inference, and taskpack actors
- [Architecture Overview](../architecture/overview.md) - How components fit into the system
