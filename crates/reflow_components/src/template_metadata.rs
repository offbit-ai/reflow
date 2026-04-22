//! Rich template metadata for Reflow node registration.
//!
//! Provides proper titles, descriptions, ports, properties, and display
//! components for all native actor templates. This replaces the empty
//! defaults that were previously registered via `get_template_mapping()`.

use reflow_network::template::{
    DisplayComponent, DisplayComponentSource, NodeShape, NodeSize, NodeTemplate,
    Port as TemplatePort, PortPosition, PortType, PropertyDefinition, PropertyType,
    PropertyValidation, RuntimeRequirements, TemplateCatalog,
};
use serde_json::json;
use std::collections::HashMap;

/// Helper to build an input port.
fn inport(id: &str, label: &str, data_type: &str) -> TemplatePort {
    TemplatePort {
        id: id.to_string(),
        label: label.to_string(),
        port_type: PortType::Input,
        position: PortPosition::Left,
        data_type: Some(data_type.to_string()),
        required: None,
        multiple: None,
    }
}

/// Helper to build an output port.
fn outport(id: &str, label: &str, data_type: &str) -> TemplatePort {
    TemplatePort {
        id: id.to_string(),
        label: label.to_string(),
        port_type: PortType::Output,
        position: PortPosition::Right,
        data_type: Some(data_type.to_string()),
        required: None,
        multiple: None,
    }
}

/// Helper to build a number property.
fn num_prop(label: &str, default: f64, min: f64, max: f64, desc: &str) -> PropertyDefinition {
    PropertyDefinition {
        property_type: PropertyType::Number,
        label: Some(label.to_string()),
        description: Some(desc.to_string()),
        default_value: Some(json!(default)),
        options: None,
        validation: Some(PropertyValidation {
            required: None,
            min: Some(min),
            max: Some(max),
            pattern: None,
        }),
    }
}

/// Helper to build a select property.
fn select_prop(label: &str, options: &[&str], default: &str, desc: &str) -> PropertyDefinition {
    PropertyDefinition {
        property_type: PropertyType::Select,
        label: Some(label.to_string()),
        description: Some(desc.to_string()),
        default_value: Some(json!(default)),
        options: Some(options.iter().map(|o| json!(o)).collect()),
        validation: None,
    }
}

/// Helper to build a string property.
fn str_prop(label: &str, default: &str, desc: &str) -> PropertyDefinition {
    PropertyDefinition {
        property_type: PropertyType::String,
        label: Some(label.to_string()),
        description: Some(desc.to_string()),
        default_value: Some(json!(default)),
        options: None,
        validation: None,
    }
}

/// Helper to build a boolean property.
fn bool_prop(label: &str, default: bool, desc: &str) -> PropertyDefinition {
    PropertyDefinition {
        property_type: PropertyType::Boolean,
        label: Some(label.to_string()),
        description: Some(desc.to_string()),
        default_value: Some(json!(default)),
        options: None,
        validation: None,
    }
}

fn props(entries: Vec<(&str, PropertyDefinition)>) -> Option<HashMap<String, PropertyDefinition>> {
    let mut map = HashMap::new();
    for (k, v) in entries {
        map.insert(k.to_string(), v);
    }
    Some(map)
}

/// Common sample rate property.
fn sample_rate_prop() -> (&'static str, PropertyDefinition) {
    (
        "sampleRate",
        num_prop(
            "Sample Rate",
            44100.0,
            8000.0,
            192000.0,
            "Audio sample rate in Hz",
        ),
    )
}

/// Build a complete NodeTemplate for a native stream actor.
fn tpl(
    id: &str,
    title: &str,
    subtitle: &str,
    category: &str,
    subcategory: &str,
    desc: &str,
    icon: &str,
    variant: &str,
    ports: Vec<TemplatePort>,
    properties: Option<HashMap<String, PropertyDefinition>>,
    version: &Option<String>,
    capabilities: &Option<Vec<String>>,
) -> NodeTemplate {
    NodeTemplate {
        id: id.to_string(),
        type_name: id.to_string(),
        title: title.to_string(),
        subtitle: Some(subtitle.to_string()),
        category: category.to_string(),
        subcategory: Some(subcategory.to_string()),
        description: desc.to_string(),
        icon: icon.to_string(),
        variant: Some(variant.to_string()),
        shape: Some(NodeShape::Rectangle),
        size: Some(NodeSize::Medium),
        ports,
        properties,
        property_rules: None,
        runtime: Some(RuntimeRequirements {
            executor: "reflow".to_string(),
            version: version.clone(),
            required_env_vars: None,
            capabilities: capabilities.clone(),
        }),
        display: None,
    }
}

/// Build a NodeTemplate with a display component.
fn tpl_display(
    id: &str,
    title: &str,
    subtitle: &str,
    category: &str,
    subcategory: &str,
    desc: &str,
    icon: &str,
    variant: &str,
    ports: Vec<TemplatePort>,
    properties: Option<HashMap<String, PropertyDefinition>>,
    display: DisplayComponent,
    version: &Option<String>,
    capabilities: &Option<Vec<String>>,
) -> NodeTemplate {
    let mut t = tpl(
        id,
        title,
        subtitle,
        category,
        subcategory,
        desc,
        icon,
        variant,
        ports,
        properties,
        version,
        capabilities,
    );
    t.display = Some(display);
    t
}

/// Creates a DisplayComponent with inline JS source.
/// Prepends the shared ReflowUI library so components can extend ReflowComponent.
fn display_inline(
    element: &str,
    source: &str,
    observed: &[&str],
    width: Option<&str>,
) -> DisplayComponent {
    // Only prepend UI lib if not already present (avoid double-loading via customElements guard)
    let full_source = format!("if(!globalThis.ReflowUI){{{}}}\n{}", UI_LIB_JS, source);
    DisplayComponent {
        element: element.to_string(),
        bundle_id: None,
        source: Some(full_source),
        shadow: Some(true),
        observed_props: Some(observed.iter().map(|s| s.to_string()).collect()),
        width: width.map(|w| w.to_string()),
    }
}

// Display component sources (compiled into binary via include_str!)
// Shared UI library — prepended to each component's inline source
const UI_LIB_JS: &str = include_str!("../../../display_components/reflow-ui.js");

const SPECTRUM_JS: &str = include_str!("../../../display_components/spectrum.js");
const DYNAMICS_JS: &str = include_str!("../../../display_components/dynamics.js");
const EQ_JS: &str = include_str!("../../../display_components/eq.js");
const STATS_JS: &str = include_str!("../../../display_components/stats.js");
const CROSSOVER_JS: &str = include_str!("../../../display_components/crossover.js");
const GAIN_JS: &str = include_str!("../../../display_components/gain.js");
const FILTER_RESPONSE_JS: &str = include_str!("../../../display_components/filter_response.js");
const BUFFER_JS: &str = include_str!("../../../display_components/buffer.js");
const IMAGE_PREVIEW_JS: &str = include_str!("../../../display_components/image_preview.js");
const WAVEFORM_JS: &str = include_str!("../../../display_components/waveform.js");
const IR_JS: &str = include_str!("../../../display_components/ir.js");
const TEXTURE_PREVIEW_JS: &str = include_str!("../../../display_components/texture_preview.js");

/// Returns display component sources for editor integrations that upload
/// component bundles separately from inline template metadata.
pub fn get_display_component_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("reflow-spectrum", SPECTRUM_JS),
        ("reflow-dynamics", DYNAMICS_JS),
        ("reflow-eq", EQ_JS),
        ("reflow-stats", STATS_JS),
        ("reflow-crossover", CROSSOVER_JS),
        ("reflow-gain", GAIN_JS),
        ("reflow-filter-response", FILTER_RESPONSE_JS),
        ("reflow-buffer", BUFFER_JS),
        ("reflow-image-preview", IMAGE_PREVIEW_JS),
        ("reflow-waveform", WAVEFORM_JS),
        ("reflow-ir", IR_JS),
        ("reflow-texture-preview", TEXTURE_PREVIEW_JS),
    ]
}

/// Returns display component sources as runtime-neutral catalog entries.
pub fn display_component_sources() -> Vec<DisplayComponentSource> {
    get_display_component_sources()
        .into_iter()
        .map(|(element, source)| DisplayComponentSource {
            element: element.to_string(),
            source: source.to_string(),
        })
        .collect()
}

/// Returns the complete native component template catalog owned by this crate.
pub fn template_catalog(
    version: &Option<String>,
    capabilities: &Option<Vec<String>>,
) -> TemplateCatalog {
    TemplateCatalog {
        templates: build_stream_actor_templates(version, capabilities),
        display_components: display_component_sources(),
    }
}

/// Returns rich template metadata for all native stream actors.
pub fn build_stream_actor_templates(
    version: &Option<String>,
    capabilities: &Option<Vec<String>>,
) -> Vec<NodeTemplate> {
    let v = version;
    let c = capabilities;

    let mut templates = vec![
        // ── Plumbing ──────────────────────────────────────────────────
        tpl(
            "tpl_bytes_to_stream",
            "Bytes to Stream",
            "Convert blob to stream",
            "stream",
            "plumbing",
            "Converts a static binary blob (Message::Bytes) into a chunked StreamHandle for incremental processing downstream.",
            "upload",
            "blue-500",
            vec![
                inport("input", "Input", "bytes"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "chunkSize",
                    num_prop(
                        "Chunk Size",
                        65536.0,
                        1024.0,
                        1048576.0,
                        "Bytes per stream chunk",
                    ),
                ),
                (
                    "contentType",
                    str_prop(
                        "Content Type",
                        "",
                        "MIME type for the stream (e.g. image/png)",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_stream_to_bytes",
            "Stream to Bytes",
            "Collect stream to blob",
            "stream",
            "plumbing",
            "Collects an entire stream into a single Message::Bytes blob. Inverse of Bytes to Stream.",
            "download",
            "blue-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("output", "Output", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_stream_tee",
            "Stream Tee",
            "Split stream 1→2",
            "stream",
            "plumbing",
            "Lossless fan-out: one input stream becomes two output streams with full backpressure on both consumers.",
            "git-branch",
            "blue-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream_a", "Stream A", "stream"),
                outport("stream_b", "Stream B", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![(
                "bufferSize",
                num_prop(
                    "Buffer Size",
                    64.0,
                    1.0,
                    4096.0,
                    "Bounded channel buffer per output",
                ),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_stream_buffer",
            "Stream Buffer",
            "Batch stream frames",
            "stream",
            "plumbing",
            "Accumulates Data frames into larger batches before forwarding. Useful for FFT windows or network efficiency.",
            "layers",
            "blue-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![(
                "bufferBytes",
                num_prop(
                    "Buffer Bytes",
                    65536.0,
                    512.0,
                    1048576.0,
                    "Bytes to accumulate before flush",
                ),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_stream_throttle",
            "Stream Throttle",
            "Rate-limit throughput",
            "stream",
            "plumbing",
            "Inserts delays between Data frames to limit throughput. Useful for real-time playback simulation.",
            "clock",
            "blue-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "delayMs",
                    num_prop(
                        "Delay (ms)",
                        10.0,
                        0.0,
                        5000.0,
                        "Milliseconds between frames",
                    ),
                ),
                (
                    "bytesPerSecond",
                    num_prop(
                        "Bytes/sec",
                        0.0,
                        0.0,
                        100000000.0,
                        "Target byte rate (0 = use delayMs)",
                    ),
                ),
            ]),
            v,
            c,
        ),
        crate::stream_ops::StreamStatsActor::actor_template(v, c),
        // ── Image DSP ─────────────────────────────────────────────────
        tpl(
            "tpl_grayscale_filter",
            "Grayscale",
            "RGBA → Gray",
            "media",
            "images",
            "Converts RGBA image stream to grayscale (Gray8) using luminance formula. SIMD-accelerated.",
            "image",
            "purple-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_brightness_contrast",
            "Brightness / Contrast",
            "Adjust image appearance",
            "media",
            "images",
            "Adjusts brightness, contrast, and saturation of an RGBA image stream per frame.",
            "sun",
            "purple-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "brightness",
                    num_prop(
                        "Brightness",
                        1.0,
                        0.0,
                        5.0,
                        "1.0 = no change, >1 = brighter",
                    ),
                ),
                (
                    "contrast",
                    num_prop(
                        "Contrast",
                        1.0,
                        0.0,
                        5.0,
                        "1.0 = no change, >1 = more contrast",
                    ),
                ),
                (
                    "saturation",
                    num_prop(
                        "Saturation",
                        1.0,
                        0.0,
                        5.0,
                        "1.0 = no change, 0 = grayscale",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_chroma_key",
            "Chroma Key",
            "Green/blue screen removal",
            "media",
            "images",
            "Removes green or blue screen background from RGBA image stream. Configurable tolerance and spill suppression.",
            "scissors",
            "purple-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "keyColor",
                    select_prop(
                        "Key Color",
                        &["green", "blue"],
                        "green",
                        "Background color to remove",
                    ),
                ),
                (
                    "tolerance",
                    num_prop(
                        "Tolerance",
                        0.3,
                        0.0,
                        1.0,
                        "Hue tolerance (higher = more aggressive)",
                    ),
                ),
                (
                    "spillSuppression",
                    num_prop(
                        "Spill Suppression",
                        0.5,
                        0.0,
                        1.0,
                        "Reduce color spill on edges",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_image_resize",
            "Image Resize",
            "Bilinear scaling",
            "media",
            "images",
            "Resizes an image stream using bilinear interpolation. Collects full image, resizes, re-emits row-by-row.",
            "maximize-2",
            "purple-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "width",
                    num_prop(
                        "Width",
                        0.0,
                        0.0,
                        16384.0,
                        "Target width in pixels (0 = keep original)",
                    ),
                ),
                (
                    "height",
                    num_prop(
                        "Height",
                        0.0,
                        0.0,
                        16384.0,
                        "Target height in pixels (0 = keep original)",
                    ),
                ),
                (
                    "channels",
                    num_prop(
                        "Channels",
                        4.0,
                        1.0,
                        4.0,
                        "Bytes per pixel (4=RGBA, 3=RGB, 1=Gray)",
                    ),
                ),
            ]),
            v,
            c,
        ),
        // ── Audio DSP — Basic ─────────────────────────────────────────
        tpl(
            "tpl_audio_gain",
            "Audio Gain",
            "Volume adjustment",
            "media",
            "audio",
            "Applies gain (volume) to PCM f32 audio stream. Supports both dB and linear gain.",
            "volume-2",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "gainDb",
                    num_prop("Gain (dB)", 0.0, -60.0, 60.0, "0=unity, 6≈double, -6≈half"),
                ),
                (
                    "gainLinear",
                    num_prop(
                        "Gain (Linear)",
                        0.0,
                        0.0,
                        100.0,
                        "Overrides dB if non-zero. 1.0=unity",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_biquad_filter",
            "Biquad Filter",
            "Configurable IIR filter",
            "media",
            "filters",
            "Second-order IIR filter supporting LowPass, HighPass, BandPass, Notch, PeakingEQ, and shelf types.",
            "activity",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "filterType",
                    select_prop(
                        "Filter Type",
                        &[
                            "lowpass",
                            "highpass",
                            "bandpass",
                            "notch",
                            "peaking",
                            "lowshelf",
                            "highshelf",
                        ],
                        "lowpass",
                        "Filter algorithm",
                    ),
                ),
                (
                    "frequency",
                    num_prop(
                        "Frequency (Hz)",
                        1000.0,
                        20.0,
                        20000.0,
                        "Cutoff or center frequency",
                    ),
                ),
                (
                    "q",
                    num_prop(
                        "Q Factor",
                        0.707,
                        0.1,
                        30.0,
                        "Resonance (0.707 = Butterworth)",
                    ),
                ),
                (
                    "gainDb",
                    num_prop(
                        "Gain (dB)",
                        0.0,
                        -24.0,
                        24.0,
                        "Boost/cut for peaking and shelf types",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_equalizer",
            "Equalizer",
            "Multi-band parametric EQ",
            "media",
            "filters",
            "Chains N biquad filters in series. Configure bands as JSON array with type, frequency, gain, and Q per band.",
            "sliders",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "bands",
                    PropertyDefinition {
                        property_type: PropertyType::CodeEditor,
                        label: Some("EQ Bands (JSON)".to_string()),
                        description: Some(
                            "Array of {type, frequency, gain, q} objects".to_string(),
                        ),
                        default_value: Some(json!([
                            {"type": "lowshelf", "frequency": 100, "gain": 0, "q": 0.707},
                            {"type": "peaking", "frequency": 1000, "gain": 0, "q": 1.0},
                            {"type": "highshelf", "frequency": 8000, "gain": 0, "q": 0.707}
                        ])),
                        options: None,
                        validation: None,
                    },
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_compressor",
            "Compressor",
            "Dynamic range compression",
            "media",
            "dynamics",
            "Reduces volume of loud signals above threshold. Configurable ratio, attack, release, knee, and makeup gain.",
            "minimize-2",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "thresholdDb",
                    num_prop(
                        "Threshold (dB)",
                        -20.0,
                        -60.0,
                        0.0,
                        "Level above which compression begins",
                    ),
                ),
                (
                    "ratio",
                    num_prop("Ratio", 4.0, 1.0, 100.0, "Compression ratio (4 = 4:1)"),
                ),
                (
                    "attackMs",
                    num_prop(
                        "Attack (ms)",
                        10.0,
                        0.01,
                        500.0,
                        "How fast compression engages",
                    ),
                ),
                (
                    "releaseMs",
                    num_prop(
                        "Release (ms)",
                        100.0,
                        1.0,
                        5000.0,
                        "How fast compression releases",
                    ),
                ),
                (
                    "kneeDb",
                    num_prop(
                        "Knee (dB)",
                        6.0,
                        0.0,
                        24.0,
                        "Soft knee width (0 = hard knee)",
                    ),
                ),
                (
                    "makeupDb",
                    num_prop("Makeup (dB)", 0.0, 0.0, 30.0, "Post-compression gain boost"),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_limiter",
            "Limiter",
            "Brickwall ceiling",
            "media",
            "dynamics",
            "Prevents audio from exceeding a ceiling. Uses infinite ratio compression for hard limiting.",
            "shield",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "ceilingDb",
                    num_prop("Ceiling (dB)", -1.0, -30.0, 0.0, "Maximum output level"),
                ),
                (
                    "attackMs",
                    num_prop("Attack (ms)", 0.1, 0.01, 10.0, "Limiter attack (very fast)"),
                ),
                (
                    "releaseMs",
                    num_prop(
                        "Release (ms)",
                        50.0,
                        1.0,
                        1000.0,
                        "How fast limiter releases",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_noise_gate",
            "Noise Gate",
            "Suppress noise floor",
            "media",
            "dynamics",
            "Attenuates audio below a threshold to eliminate background noise. Companion to Compressor.",
            "volume-x",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "thresholdDb",
                    num_prop(
                        "Threshold (dB)",
                        -40.0,
                        -80.0,
                        0.0,
                        "Level below which gating occurs",
                    ),
                ),
                (
                    "ratio",
                    num_prop("Ratio", 10.0, 1.0, 100.0, "Expansion ratio (∞ = hard gate)"),
                ),
                (
                    "attackMs",
                    num_prop("Attack (ms)", 1.0, 0.01, 100.0, "How fast gate opens"),
                ),
                (
                    "releaseMs",
                    num_prop("Release (ms)", 50.0, 1.0, 2000.0, "How fast gate closes"),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_de_esser",
            "De-Esser",
            "Reduce sibilance",
            "media",
            "dynamics",
            "Frequency-selective compressor targeting sibilance (4–8 kHz). Tames harsh 's' and 't' sounds in speech.",
            "mic-off",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "frequency",
                    num_prop(
                        "Center Frequency (Hz)",
                        6000.0,
                        2000.0,
                        12000.0,
                        "Sibilance detection band center",
                    ),
                ),
                (
                    "thresholdDb",
                    num_prop(
                        "Threshold (dB)",
                        -20.0,
                        -60.0,
                        0.0,
                        "Level above which de-essing activates",
                    ),
                ),
                (
                    "ratio",
                    num_prop(
                        "Ratio",
                        4.0,
                        1.0,
                        20.0,
                        "Compression ratio on sibilant band",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_dc_offset",
            "DC Offset Removal",
            "Remove DC bias",
            "media",
            "audio",
            "Removes DC offset from audio via single-pole high-pass filter at very low frequency (default 5 Hz).",
            "minus-circle",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "cutoffHz",
                    num_prop(
                        "Cutoff (Hz)",
                        5.0,
                        1.0,
                        50.0,
                        "HPF cutoff — only removes DC/sub-infra",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_audio_normalize",
            "Normalize",
            "Peak normalization",
            "media",
            "audio",
            "Two-pass peak normalization. Scans for peak level, then applies gain to reach target.",
            "maximize",
            "green-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![(
                "targetDb",
                num_prop(
                    "Target (dB)",
                    -1.0,
                    -30.0,
                    0.0,
                    "Target peak level (0 = full scale)",
                ),
            )]),
            v,
            c,
        ),
        // ── Audio DSP — Analysis ──────────────────────────────────────
        tpl(
            "tpl_audio_spectrum",
            "Spectrum Analyzer",
            "FFT visualization",
            "media",
            "analysis",
            "Windowed FFT producing magnitude bins as audio/frequency-bins stream. Designed for Zeal spectrum display.",
            "bar-chart",
            "amber-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Spectrum", "stream"),
                outport("stats", "Stats", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "fftSize",
                    num_prop(
                        "FFT Size",
                        2048.0,
                        256.0,
                        16384.0,
                        "Window size (power of 2)",
                    ),
                ),
                (
                    "hopSize",
                    num_prop(
                        "Hop Size",
                        512.0,
                        64.0,
                        8192.0,
                        "Samples between FFT frames",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_envelope_follower",
            "Envelope Follower",
            "Amplitude tracking",
            "media",
            "analysis",
            "Extracts amplitude envelope as control-signal stream. For sidechain, ducking, modulation, visualization.",
            "trending-up",
            "amber-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Envelope", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "attackMs",
                    num_prop("Attack (ms)", 5.0, 0.1, 500.0, "Envelope attack time"),
                ),
                (
                    "releaseMs",
                    num_prop("Release (ms)", 50.0, 1.0, 5000.0, "Envelope release time"),
                ),
                (
                    "mode",
                    select_prop(
                        "Detection Mode",
                        &["peak", "rms"],
                        "peak",
                        "Peak or RMS level detection",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_silence_detect",
            "Silence Detect",
            "Voice activity detection",
            "media",
            "analysis",
            "Monitors audio level and emits events when silence begins/ends. For automatic recording, skipping dead air.",
            "mic-off",
            "amber-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("events", "Events", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "thresholdDb",
                    num_prop(
                        "Threshold (dB)",
                        -40.0,
                        -80.0,
                        0.0,
                        "Level below which is silence",
                    ),
                ),
                (
                    "minDurationMs",
                    num_prop(
                        "Min Duration (ms)",
                        500.0,
                        50.0,
                        10000.0,
                        "Minimum silence duration to trigger",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_peak_detect",
            "Peak Detect",
            "Onset/transient detection",
            "media",
            "analysis",
            "Detects transients (sudden energy increases) for beat tracking, segmentation, and triggering.",
            "zap",
            "amber-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("events", "Events", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "sensitivity",
                    num_prop(
                        "Sensitivity",
                        3.0,
                        1.0,
                        20.0,
                        "Transient must exceed N× running average",
                    ),
                ),
                (
                    "windowMs",
                    num_prop("Window (ms)", 50.0, 5.0, 500.0, "Running average window"),
                ),
                (
                    "minIntervalMs",
                    num_prop(
                        "Min Interval (ms)",
                        100.0,
                        10.0,
                        5000.0,
                        "Minimum time between peaks",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        // ── Audio DSP — Spectral / Advanced ───────────────────────────
        tpl(
            "tpl_ifft",
            "Inverse FFT",
            "Frequency → time domain",
            "media",
            "spectral",
            "Converts frequency-domain magnitude bins back to PCM audio via overlap-add ISTFT.",
            "refresh-ccw",
            "orange-500",
            vec![
                inport("stream", "Spectrum", "stream"),
                outport("stream", "Audio", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "fftSize",
                    num_prop(
                        "FFT Size",
                        2048.0,
                        256.0,
                        16384.0,
                        "Must match upstream FFT size",
                    ),
                ),
                (
                    "hopSize",
                    num_prop(
                        "Hop Size",
                        512.0,
                        64.0,
                        8192.0,
                        "Must match upstream hop size",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_convolve",
            "Convolution",
            "FIR / impulse response",
            "media",
            "spectral",
            "Convolves audio stream with an impulse response for reverb or FIR filtering. Impulse arrives as Bytes.",
            "radio",
            "orange-500",
            vec![
                inport("stream", "Audio", "stream"),
                inport("impulse", "Impulse Response", "bytes"),
                outport("stream", "Output", "stream"),
                outport("error", "Error", "string"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_noise_reduction",
            "Noise Reduction",
            "Spectral subtraction",
            "media",
            "spectral",
            "Learns noise profile from initial silence, then subtracts it from all subsequent frames.",
            "wind",
            "orange-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "fftSize",
                    num_prop("FFT Size", 2048.0, 256.0, 16384.0, "Analysis window size"),
                ),
                (
                    "hopSize",
                    num_prop("Hop Size", 512.0, 64.0, 8192.0, "Hop between frames"),
                ),
                (
                    "profileMs",
                    num_prop(
                        "Profile Duration (ms)",
                        500.0,
                        100.0,
                        5000.0,
                        "Initial silence to learn noise from",
                    ),
                ),
                (
                    "strength",
                    num_prop(
                        "Strength",
                        1.0,
                        0.0,
                        3.0,
                        "Subtraction strength (1.0 = full)",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_pitch_shift",
            "Pitch Shift",
            "Change pitch without speed",
            "media",
            "spectral",
            "Phase vocoder pitch shifting. Shifts pitch by semitones while preserving duration.",
            "music",
            "orange-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "semitones",
                    num_prop(
                        "Semitones",
                        0.0,
                        -24.0,
                        24.0,
                        "Pitch shift amount (+12 = up one octave)",
                    ),
                ),
                (
                    "fftSize",
                    num_prop(
                        "FFT Size",
                        4096.0,
                        1024.0,
                        16384.0,
                        "Analysis window (larger = better quality)",
                    ),
                ),
                (
                    "hopSize",
                    num_prop("Hop Size", 256.0, 64.0, 4096.0, "Samples between frames"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_time_stretch",
            "Time Stretch",
            "Change speed without pitch",
            "media",
            "spectral",
            "WSOLA time stretching. Changes duration without affecting pitch.",
            "fast-forward",
            "orange-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("stream", "Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "ratio",
                    num_prop("Ratio", 1.0, 0.25, 4.0, "2.0 = double duration, 0.5 = half"),
                ),
                (
                    "windowSize",
                    num_prop(
                        "Window Size",
                        1024.0,
                        256.0,
                        8192.0,
                        "WSOLA analysis window",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_crossover",
            "Crossover",
            "Multi-band frequency split",
            "media",
            "spectral",
            "3-way Linkwitz-Riley crossover splitting audio into low, mid, and high frequency bands.",
            "git-merge",
            "orange-500",
            vec![
                inport("stream", "Stream", "stream"),
                outport("low", "Low", "stream"),
                outport("mid", "Mid", "stream"),
                outport("high", "High", "stream"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "lowFrequency",
                    num_prop(
                        "Low Crossover (Hz)",
                        200.0,
                        20.0,
                        2000.0,
                        "Low/mid split frequency",
                    ),
                ),
                (
                    "highFrequency",
                    num_prop(
                        "High Crossover (Hz)",
                        4000.0,
                        500.0,
                        16000.0,
                        "Mid/high split frequency",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_correlator",
            "Correlator",
            "Cross-correlation analysis",
            "media",
            "analysis",
            "Computes normalized cross-correlation between two audio streams for delay estimation and similarity.",
            "link-2",
            "amber-500",
            vec![
                inport("stream_a", "Stream A", "stream"),
                inport("stream_b", "Stream B", "stream"),
                outport("stream", "Correlation", "stream"),
                outport("stats", "Stats", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "maxLagMs",
                    num_prop(
                        "Max Lag (ms)",
                        100.0,
                        1.0,
                        5000.0,
                        "Maximum search lag for correlation",
                    ),
                ),
                sample_rate_prop(),
            ]),
            v,
            c,
        ),
        // ── Procedural ───────────────────────────────────────────────
        tpl(
            "tpl_noise_generator",
            "Noise Generator",
            "Procedural noise field",
            "media",
            "procedural",
            "Generates a 2D noise grid. Supports Perlin, Simplex, Worley, Value, Ridged, and White noise with FBM layering.",
            "waves",
            "cyan-500",
            vec![
                inport("scale", "Scale", "float"),
                inport("seed", "Seed", "float"),
                inport("offsetX", "Offset X", "float"),
                inport("offsetY", "Offset Y", "float"),
                outport("output", "Grid", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "noiseType",
                    select_prop(
                        "Noise Type",
                        &["perlin", "simplex", "worley", "value", "ridged", "white"],
                        "perlin",
                        "Noise algorithm",
                    ),
                ),
                (
                    "width",
                    num_prop("Width", 256.0, 16.0, 4096.0, "Grid width in samples"),
                ),
                (
                    "height",
                    num_prop("Height", 256.0, 16.0, 4096.0, "Grid height in samples"),
                ),
                (
                    "scale",
                    num_prop(
                        "Scale",
                        4.0,
                        0.1,
                        100.0,
                        "Frequency scale (higher = more detail)",
                    ),
                ),
                (
                    "octaves",
                    num_prop(
                        "Octaves",
                        6.0,
                        1.0,
                        12.0,
                        "FBM layers (more = finer detail)",
                    ),
                ),
                (
                    "lacunarity",
                    num_prop(
                        "Lacunarity",
                        2.0,
                        1.0,
                        4.0,
                        "Frequency multiplier per octave",
                    ),
                ),
                (
                    "persistence",
                    num_prop("Persistence", 0.5, 0.0, 1.0, "Amplitude decay per octave"),
                ),
                (
                    "seed",
                    num_prop("Seed", 0.0, -1000.0, 1000.0, "Random seed offset"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_image_to_heightmap",
            "Image to Heightmap",
            "Extract height data from image",
            "media",
            "procedural",
            "Converts an image to a float height grid using luminance (0.299R + 0.587G + 0.114B). Accepts encoded images or raw RGBA bytes.",
            "mountain",
            "cyan-500",
            vec![
                inport("input", "Image", "bytes"),
                outport("output", "Grid", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_heightmap_to_image",
            "Heightmap to Image",
            "Render height grid as image",
            "media",
            "procedural",
            "Converts a float height grid to RGBA pixels. Supports grayscale and terrain color modes (water→grass→mountain→snow).",
            "image",
            "cyan-500",
            vec![
                inport("input", "Grid", "bytes"),
                outport("output", "Image", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "colorMode",
                    select_prop(
                        "Color Mode",
                        &["grayscale", "terrain"],
                        "grayscale",
                        "Rendering style",
                    ),
                ),
                (
                    "width",
                    num_prop(
                        "Width",
                        256.0,
                        16.0,
                        4096.0,
                        "Grid width (if not auto-detected)",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_heightmap_to_mesh",
            "Heightmap to Mesh",
            "Generate terrain mesh",
            "media",
            "procedural",
            "Generates a triangle mesh from a height grid with positions, normals, and UVs. Output is interleaved vertex data (pos3+normal3+uv2) + index buffer.",
            "box",
            "cyan-500",
            vec![
                inport("input", "Grid", "bytes"),
                outport("mesh", "Mesh", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "heightScale",
                    num_prop("Height Scale", 10.0, 0.1, 100.0, "Vertical exaggeration"),
                ),
                (
                    "meshWidth",
                    num_prop("Mesh Width", 10.0, 1.0, 1000.0, "World units width"),
                ),
                (
                    "meshDepth",
                    num_prop("Mesh Depth", 10.0, 1.0, 1000.0, "World units depth"),
                ),
                (
                    "width",
                    num_prop(
                        "Grid Width",
                        256.0,
                        16.0,
                        4096.0,
                        "Height grid width (if not auto-detected)",
                    ),
                ),
            ]),
            v,
            c,
        ),
        // ── SDF Primitives ───────────────────────────────────────────
        tpl(
            "tpl_sdf_sphere",
            "SDF Sphere",
            "Sphere primitive",
            "3d",
            "sdf",
            "Signed distance sphere with configurable radius.",
            "circle",
            "violet-500",
            vec![
                inport("trigger", "Trigger", "any"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![(
                "radius",
                num_prop("Radius", 1.0, 0.01, 100.0, "Sphere radius"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_box",
            "SDF Box",
            "Box primitive",
            "3d",
            "sdf",
            "Signed distance box with configurable dimensions.",
            "square",
            "violet-500",
            vec![
                inport("trigger", "Trigger", "any"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                ("sizeX", num_prop("Width", 1.0, 0.01, 100.0, "X dimension")),
                ("sizeY", num_prop("Height", 1.0, 0.01, 100.0, "Y dimension")),
                ("sizeZ", num_prop("Depth", 1.0, 0.01, 100.0, "Z dimension")),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_torus",
            "SDF Torus",
            "Torus primitive",
            "3d",
            "sdf",
            "Signed distance torus on XZ plane.",
            "circle",
            "violet-500",
            vec![
                inport("trigger", "Trigger", "any"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                (
                    "majorRadius",
                    num_prop("Major Radius", 1.0, 0.1, 50.0, "Ring radius"),
                ),
                (
                    "minorRadius",
                    num_prop("Minor Radius", 0.3, 0.01, 10.0, "Tube radius"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_cylinder",
            "SDF Cylinder",
            "Cylinder primitive",
            "3d",
            "sdf",
            "Signed distance cylinder along Y axis.",
            "circle",
            "violet-500",
            vec![
                inport("trigger", "Trigger", "any"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                (
                    "radius",
                    num_prop("Radius", 0.5, 0.01, 50.0, "Cylinder radius"),
                ),
                (
                    "height",
                    num_prop("Height", 1.0, 0.01, 100.0, "Half-height"),
                ),
            ]),
            v,
            c,
        ),
        // ── SDF Operations ───────────────────────────────────────────
        tpl(
            "tpl_sdf_smooth_union",
            "Smooth Union",
            "Blend two SDFs",
            "3d",
            "sdf",
            "Smooth boolean union with configurable blend radius.",
            "git-merge",
            "violet-500",
            vec![
                inport("sdf_a", "A", "object"),
                inport("sdf_b", "B", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![(
                "smoothness",
                num_prop("Smoothness", 0.3, 0.0, 5.0, "Blend radius (k)"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_union",
            "Union",
            "Boolean union (min)",
            "3d",
            "sdf",
            "Hard boolean union of two SDFs.",
            "git-merge",
            "violet-500",
            vec![
                inport("sdf_a", "A", "object"),
                inport("sdf_b", "B", "object"),
                outport("sdf", "SDF", "object"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_sdf_difference",
            "Difference",
            "Boolean subtract",
            "3d",
            "sdf",
            "Subtract SDF B from SDF A.",
            "minus-circle",
            "violet-500",
            vec![
                inport("sdf_a", "A", "object"),
                inport("sdf_b", "B", "object"),
                outport("sdf", "SDF", "object"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_sdf_intersection",
            "Intersection",
            "Boolean intersect",
            "3d",
            "sdf",
            "Intersection of two SDFs.",
            "crosshair",
            "violet-500",
            vec![
                inport("sdf_a", "A", "object"),
                inport("sdf_b", "B", "object"),
                outport("sdf", "SDF", "object"),
            ],
            None,
            v,
            c,
        ),
        // ── SDF Transforms ───────────────────────────────────────────
        tpl(
            "tpl_sdf_translate",
            "Translate",
            "Move SDF",
            "3d",
            "sdf",
            "Translate SDF in world space.",
            "move",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                ("x", num_prop("X", 0.0, -100.0, 100.0, "X offset")),
                ("y", num_prop("Y", 0.0, -100.0, 100.0, "Y offset")),
                ("z", num_prop("Z", 0.0, -100.0, 100.0, "Z offset")),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_rotate",
            "Rotate",
            "Rotate SDF",
            "3d",
            "sdf",
            "Rotate SDF by Euler angles (degrees).",
            "rotate-cw",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                ("x", num_prop("X°", 0.0, -360.0, 360.0, "X rotation")),
                ("y", num_prop("Y°", 0.0, -360.0, 360.0, "Y rotation")),
                ("z", num_prop("Z°", 0.0, -360.0, 360.0, "Z rotation")),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_twist",
            "Twist",
            "Twist deformation",
            "3d",
            "sdf",
            "Twist SDF around Y axis.",
            "rotate-ccw",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![(
                "strength",
                num_prop("Strength", 0.5, -10.0, 10.0, "Radians per unit"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_mirror",
            "Mirror",
            "Mirror across axis",
            "3d",
            "sdf",
            "Mirror SDF across one or more axes.",
            "flip-horizontal",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                ("axisX", num_prop("X", 1.0, 0.0, 1.0, "Mirror X (1=yes)")),
                ("axisY", num_prop("Y", 0.0, 0.0, 1.0, "Mirror Y")),
                ("axisZ", num_prop("Z", 0.0, 0.0, 1.0, "Mirror Z")),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_repeat",
            "Repeat",
            "Finite repetition",
            "3d",
            "sdf",
            "Repeat SDF in a grid pattern.",
            "grid",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                (
                    "spacingX",
                    num_prop("Spacing X", 2.0, 0.1, 50.0, "X spacing"),
                ),
                (
                    "spacingY",
                    num_prop("Spacing Y", 2.0, 0.1, 50.0, "Y spacing"),
                ),
                (
                    "spacingZ",
                    num_prop("Spacing Z", 2.0, 0.1, 50.0, "Z spacing"),
                ),
                (
                    "countX",
                    num_prop("Count X", 3.0, 1.0, 20.0, "X repetitions"),
                ),
                (
                    "countY",
                    num_prop("Count Y", 1.0, 1.0, 20.0, "Y repetitions"),
                ),
                (
                    "countZ",
                    num_prop("Count Z", 3.0, 1.0, 20.0, "Z repetitions"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_displace",
            "Displace",
            "Noise displacement",
            "3d",
            "sdf",
            "Add FBM noise displacement to SDF surface.",
            "waves",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("sdf", "SDF", "object"),
            ],
            props(vec![
                (
                    "frequency",
                    num_prop("Frequency", 3.0, 0.1, 50.0, "Noise frequency"),
                ),
                (
                    "amplitude",
                    num_prop("Amplitude", 0.1, 0.0, 2.0, "Displacement amount"),
                ),
                ("octaves", num_prop("Octaves", 4.0, 1.0, 8.0, "FBM layers")),
            ]),
            v,
            c,
        ),
        // ── GPU Compute ──────────────────────────────────────────────
        tpl(
            "tpl_sdf_render",
            "SDF Render",
            "GPU ray march to image",
            "3d",
            "sdf",
            "Compiles SDF to WGSL, renders via GPU compute shader, outputs RGBA bytes.",
            "monitor",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("output", "Image", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "width",
                    num_prop("Width", 512.0, 64.0, 4096.0, "Output width"),
                ),
                (
                    "height",
                    num_prop("Height", 512.0, 64.0, 4096.0, "Output height"),
                ),
                (
                    "maxSteps",
                    num_prop("Max Steps", 128.0, 16.0, 512.0, "Ray march iterations"),
                ),
                (
                    "fov",
                    num_prop("FOV", 45.0, 10.0, 120.0, "Field of view (degrees)"),
                ),
                ("ao", bool_prop("Ambient Occlusion", true, "Enable AO")),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_sdf_marching_cubes",
            "Marching Cubes",
            "SDF → triangle mesh (GPU)",
            "3d",
            "sdf",
            "Extracts triangle mesh from SDF via GPU marching cubes.",
            "box",
            "violet-500",
            vec![
                inport("sdf", "SDF", "object"),
                outport("mesh", "Mesh", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "resolution",
                    num_prop("Resolution", 64.0, 8.0, 256.0, "Grid density"),
                ),
                ("bound", num_prop("Bound", 2.5, 0.5, 50.0, "World extent")),
                (
                    "isoLevel",
                    num_prop("Iso Level", 0.0, -1.0, 1.0, "Surface threshold"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_mesh_to_sdf",
            "Mesh to SDF",
            "Triangle mesh → distance field (GPU)",
            "3d",
            "sdf",
            "Computes unsigned distance field from triangle mesh via GPU.",
            "box",
            "violet-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                outport("output", "Volume", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "resolution",
                    num_prop("Resolution", 32.0, 8.0, 128.0, "Volume grid density"),
                ),
                ("bound", num_prop("Bound", 3.0, 0.5, 50.0, "World extent")),
            ]),
            v,
            c,
        ),
        // ── Mesh Export ──────────────────────────────────────────────
        tpl(
            "tpl_obj_export",
            "OBJ Export",
            "Wavefront OBJ",
            "3d",
            "export",
            "Converts mesh bytes to Wavefront OBJ text format.",
            "file-text",
            "slate-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                outport("output", "OBJ", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![(
                "name",
                str_prop("Object Name", "mesh", "Name in OBJ file"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_stl_export",
            "STL Export",
            "Binary STL",
            "3d",
            "export",
            "Converts mesh bytes to binary STL format.",
            "file",
            "slate-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                outport("output", "STL", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_gltf_export",
            "glTF Export",
            "GLB binary",
            "3d",
            "export",
            "Converts mesh bytes to self-contained glTF 2.0 binary (.glb).",
            "file",
            "slate-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                outport("output", "GLB", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![(
                "name",
                str_prop("Mesh Name", "mesh", "Name in glTF scene"),
            )]),
            v,
            c,
        ),
        // ── File I/O ─────────────────────────────────────────────────
        tpl(
            "tpl_file_load",
            "File Load",
            "Read file to bytes",
            "tools-utilities",
            "files",
            "Reads a file from disk into Message::Bytes with MIME detection.",
            "upload",
            "slate-500",
            vec![
                inport("trigger", "Trigger", "any"),
                outport("output", "Data", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![("path", str_prop("Path", "", "File path to read"))]),
            v,
            c,
        ),
        tpl(
            "tpl_file_save",
            "File Save",
            "Write bytes to file",
            "tools-utilities",
            "files",
            "Writes Message::Bytes to a file on disk.",
            "download",
            "slate-500",
            vec![
                inport("input", "Data", "bytes"),
                outport("path", "Path", "string"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                ("path", str_prop("Path", "", "File path to write")),
                (
                    "createDirs",
                    bool_prop("Create Dirs", true, "Auto-create parent directories"),
                ),
            ]),
            v,
            c,
        ),
        // ── Texture Mapping ──────────────────────────────────────────
        tpl(
            "tpl_triplanar_texture",
            "Triplanar Texture",
            "Project texture via normals",
            "3d",
            "transforms",
            "Projects a texture onto mesh vertices using triplanar mapping. Blends XY/XZ/YZ projections weighted by surface normal. No UVs needed.",
            "image",
            "violet-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                inport("texture", "Texture", "bytes"),
                outport("mesh", "Textured Mesh", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "scale",
                    num_prop("Scale", 1.0, 0.01, 100.0, "Texture tiling scale"),
                ),
                (
                    "sharpness",
                    num_prop(
                        "Sharpness",
                        2.0,
                        0.1,
                        10.0,
                        "Blend falloff between projections",
                    ),
                ),
                (
                    "stride",
                    num_prop(
                        "Input Stride",
                        24.0,
                        12.0,
                        128.0,
                        "Bytes per vertex in input mesh",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_uv_texture",
            "UV Texture",
            "Sample texture at UVs",
            "3d",
            "transforms",
            "Samples a texture at mesh UV coordinates and appends vertex colors. Requires mesh with UV data.",
            "image",
            "violet-500",
            vec![
                inport("mesh", "Mesh", "bytes"),
                inport("texture", "Texture", "bytes"),
                outport("mesh", "Textured Mesh", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "stride",
                    num_prop(
                        "Input Stride",
                        32.0,
                        12.0,
                        128.0,
                        "Bytes per vertex in input mesh",
                    ),
                ),
                (
                    "uvOffset",
                    num_prop(
                        "UV Offset",
                        6.0,
                        0.0,
                        32.0,
                        "Float index where UVs start in vertex",
                    ),
                ),
            ]),
            v,
            c,
        ),
        // ── Flow Control ─────────────────────────────────────────────
        tpl(
            "tpl_map",
            "Map",
            "Transform each array item",
            "logic-control",
            "conditions",
            "Applies an operation to each item in an array.",
            "list",
            "indigo-500",
            vec![
                inport("input", "Array", "array"),
                outport("output", "Array", "array"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "operation",
                    select_prop(
                        "Operation",
                        &["identity", "to_string", "to_number", "extract_field"],
                        "identity",
                        "Transform to apply",
                    ),
                ),
                (
                    "field",
                    str_prop("Field", "", "Field name for extract_field operation"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_filter",
            "Filter",
            "Keep items by condition",
            "logic-control",
            "conditions",
            "Filters an array, keeping items that match a condition. Rejected items output on the rejected port.",
            "filter",
            "indigo-500",
            vec![
                inport("input", "Array", "array"),
                outport("output", "Passed", "array"),
                outport("rejected", "Rejected", "array"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "condition",
                    select_prop(
                        "Condition",
                        &[
                            "truthy",
                            "not_null",
                            "equals",
                            "greater_than",
                            "less_than",
                            "contains",
                        ],
                        "truthy",
                        "Filter condition",
                    ),
                ),
                (
                    "field",
                    str_prop(
                        "Field",
                        "",
                        "Nested field to test (empty = test item directly)",
                    ),
                ),
                (
                    "value",
                    str_prop("Compare Value", "", "Value to compare against"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_reduce",
            "Reduce",
            "Accumulate to single value",
            "logic-control",
            "conditions",
            "Reduces an array to a single value using an aggregation operation.",
            "minimize-2",
            "indigo-500",
            vec![
                inport("input", "Array", "array"),
                outport("output", "Result", "any"),
                outport("error", "Error", "string"),
            ],
            props(vec![(
                "operation",
                select_prop(
                    "Operation",
                    &[
                        "sum", "product", "min", "max", "count", "concat", "first", "last",
                    ],
                    "sum",
                    "Reduce operation",
                ),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_merge",
            "Merge",
            "Combine inputs",
            "logic-control",
            "conditions",
            "Merges multiple inputs into a single object or array.",
            "git-merge",
            "indigo-500",
            vec![
                inport("a", "A", "any"),
                inport("b", "B", "any"),
                inport("c", "C", "any"),
                inport("d", "D", "any"),
                outport("output", "Merged", "any"),
            ],
            props(vec![(
                "mode",
                select_prop(
                    "Mode",
                    &["object", "array"],
                    "object",
                    "Output as object or array",
                ),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_split",
            "Split",
            "Head + tail decomposition",
            "logic-control",
            "conditions",
            "Splits an array into first element (head) and remaining (tail).",
            "scissors",
            "indigo-500",
            vec![
                inport("input", "Array", "array"),
                outport("head", "Head", "any"),
                outport("tail", "Tail", "array"),
                outport("count", "Count", "integer"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_gate",
            "Gate",
            "Conditional pass/block",
            "logic-control",
            "conditions",
            "Passes or blocks data based on a boolean control signal.",
            "toggle-left",
            "indigo-500",
            vec![
                inport("input", "Data", "any"),
                inport("control", "Control", "boolean"),
                outport("output", "Passed", "any"),
                outport("blocked", "Blocked", "any"),
            ],
            props(vec![(
                "invert",
                bool_prop("Invert", false, "Invert the gate (block when true)"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_delay",
            "Delay",
            "Timed passthrough",
            "logic-control",
            "conditions",
            "Delays data forwarding by a configurable duration.",
            "clock",
            "indigo-500",
            vec![
                inport("input", "Data", "any"),
                outport("output", "Data", "any"),
            ],
            props(vec![(
                "delayMs",
                num_prop("Delay (ms)", 1000.0, 0.0, 60000.0, "Delay in milliseconds"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_collect",
            "Collect",
            "Accumulate N items",
            "logic-control",
            "conditions",
            "Collects N incoming messages into an array, then emits.",
            "inbox",
            "indigo-500",
            vec![
                inport("input", "Item", "any"),
                outport("output", "Array", "array"),
                outport("count", "Count", "integer"),
            ],
            props(vec![(
                "count",
                num_prop("Count", 10.0, 1.0, 10000.0, "Number of items to collect"),
            )]),
            v,
            c,
        ),
        tpl(
            "tpl_passthrough",
            "Passthrough",
            "Identity / debug tap",
            "logic-control",
            "conditions",
            "Passes data through unchanged. Useful for debugging and observation points.",
            "arrow-right",
            "indigo-500",
            vec![
                inport("input", "Data", "any"),
                outport("output", "Data", "any"),
            ],
            None,
            v,
            c,
        ),
        // ── Triggers ─────────────────────────────────────────────────
        tpl(
            "tpl_interval_trigger",
            "Interval Trigger",
            "Periodic timer",
            "tools-utilities",
            "triggers",
            "Emits a trigger signal at regular intervals.",
            "clock",
            "purple-600",
            vec![
                inport("start", "Start", "any"),
                outport("trigger", "Trigger", "object"),
            ],
            props(vec![
                (
                    "interval",
                    num_prop("Interval", 60000.0, 1000.0, 86400000.0, "Interval value"),
                ),
                (
                    "intervalUnit",
                    select_prop(
                        "Unit",
                        &["milliseconds", "seconds", "minutes", "hours", "days"],
                        "milliseconds",
                        "Interval unit",
                    ),
                ),
                (
                    "startImmediately",
                    bool_prop("Start Immediately", true, "Trigger on workflow start"),
                ),
                (
                    "maxExecutions",
                    num_prop("Max Executions", 0.0, 0.0, 1000000.0, "0 = unlimited"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_cron_trigger",
            "Cron Trigger",
            "Schedule-based timer",
            "tools-utilities",
            "triggers",
            "Emits a trigger signal based on a cron expression.",
            "calendar",
            "blue-600",
            vec![
                inport("start", "Start", "any"),
                outport("trigger", "Trigger", "object"),
            ],
            props(vec![
                (
                    "commonSchedules",
                    select_prop(
                        "Schedule",
                        &[
                            "Custom",
                            "Every minute",
                            "Every 5 minutes",
                            "Every 15 minutes",
                            "Every 30 minutes",
                            "Every hour",
                            "Every day at midnight",
                            "Every Monday at 9 AM",
                            "First day of month",
                        ],
                        "Custom",
                        "Common schedule presets",
                    ),
                ),
                (
                    "cronExpression",
                    str_prop(
                        "Cron Expression",
                        "0 * * * *",
                        "Custom cron (when Schedule is Custom)",
                    ),
                ),
                (
                    "maxExecutions",
                    num_prop("Max Executions", 0.0, 0.0, 1000000.0, "0 = unlimited"),
                ),
            ]),
            v,
            c,
        ),
        // ── Server ───────────────────────────────────────────────────
        tpl(
            "tpl_server_request",
            "Server Request",
            "Webhook entry point",
            "tools-utilities",
            "triggers",
            "Entry point for webhook-triggered workflows. The server injects the HTTP request data into this actor's config at execution time.",
            "globe",
            "purple-600",
            vec![
                outport("body", "Body", "object"),
                outport("headers", "Headers", "object"),
                outport("params", "Params", "object"),
                outport("method", "Method", "string"),
                outport("url", "URL", "string"),
            ],
            props(vec![
                (
                    "path",
                    str_prop("Path", "/webhook", "Webhook route to listen on"),
                ),
                (
                    "method",
                    select_prop(
                        "Method",
                        &["GET", "POST", "PUT", "PATCH", "DELETE"],
                        "POST",
                        "HTTP method filter",
                    ),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_server_response",
            "Server Response",
            "Webhook response",
            "tools-utilities",
            "triggers",
            "Constructs an HTTP response to send back to the webhook caller.",
            "send",
            "purple-600",
            vec![
                inport("body", "Body", "any"),
                inport("status", "Status", "integer"),
                inport("headers", "Headers", "object"),
                outport("response", "Response", "object"),
            ],
            props(vec![
                (
                    "statusCode",
                    num_prop("Status Code", 200.0, 100.0, 599.0, "HTTP status code"),
                ),
                (
                    "contentType",
                    select_prop(
                        "Content Type",
                        &["application/json", "text/plain", "text/html"],
                        "application/json",
                        "Response content type",
                    ),
                ),
            ]),
            v,
            c,
        ),
        // ── Procedural (continued) ───────────────────────────────────
        tpl(
            "tpl_voronoi",
            "Voronoi",
            "Voronoi diagram",
            "media",
            "procedural",
            "Generates a 2D Voronoi diagram with distance and cell ID grids.",
            "hexagon",
            "cyan-500",
            vec![
                inport("seed", "Seed", "float"),
                outport("distance", "Distance", "bytes"),
                outport("cell_id", "Cell ID", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "width",
                    num_prop("Width", 256.0, 16.0, 4096.0, "Grid width"),
                ),
                (
                    "height",
                    num_prop("Height", 256.0, 16.0, 4096.0, "Grid height"),
                ),
                (
                    "cellCount",
                    num_prop("Cells", 32.0, 2.0, 1000.0, "Number of Voronoi cells"),
                ),
                (
                    "seed",
                    num_prop("Seed", 0.0, -1000.0, 1000.0, "Random seed"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_lsystem",
            "L-System",
            "Lindenmayer string rewriting",
            "media",
            "procedural",
            "Generates fractal patterns via string rewriting rules with turtle graphics interpretation.",
            "git-branch",
            "cyan-500",
            vec![
                inport("axiom", "Axiom", "string"),
                outport("output", "String", "string"),
                outport("points", "Points", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                ("axiom", str_prop("Axiom", "F", "Starting string")),
                (
                    "rules",
                    str_prop(
                        "Rules",
                        "F=F+F-F-FF+F+F-F",
                        "Production rules (semicolon-separated, e.g. F=F+F;G=GG)",
                    ),
                ),
                (
                    "iterations",
                    num_prop("Iterations", 4.0, 1.0, 8.0, "Number of rewriting passes"),
                ),
                (
                    "angle",
                    num_prop("Angle", 25.0, 0.0, 360.0, "Turn angle in degrees"),
                ),
                (
                    "stepLength",
                    num_prop("Step Length", 1.0, 0.01, 10.0, "Forward step distance"),
                ),
            ]),
            v,
            c,
        ),
        tpl(
            "tpl_particle_emitter",
            "Particle Emitter",
            "Generate particle set",
            "media",
            "procedural",
            "Generates a set of particles with position, velocity, and lifetime. Configurable emission shape.",
            "sparkles",
            "cyan-500",
            vec![
                inport("trigger", "Trigger", "any"),
                inport("count", "Count", "integer"),
                outport("particles", "Particles", "bytes"),
                outport("metadata", "Metadata", "object"),
            ],
            props(vec![
                (
                    "count",
                    num_prop("Count", 1000.0, 1.0, 100000.0, "Number of particles"),
                ),
                (
                    "shape",
                    select_prop(
                        "Shape",
                        &["sphere", "cube", "disc"],
                        "sphere",
                        "Emission shape",
                    ),
                ),
                (
                    "radius",
                    num_prop("Radius", 1.0, 0.01, 100.0, "Emission radius"),
                ),
                (
                    "speed",
                    num_prop("Speed", 1.0, 0.0, 100.0, "Initial velocity magnitude"),
                ),
                (
                    "lifetime",
                    num_prop("Lifetime", 5.0, 0.1, 60.0, "Particle lifetime in seconds"),
                ),
                (
                    "seed",
                    num_prop("Seed", 0.0, -1000.0, 1000.0, "Random seed"),
                ),
            ]),
            v,
            c,
        ),
        // ── Image Codecs ─────────────────────────────────────────────
        tpl(
            "tpl_image_decode",
            "Image Decode",
            "Decode PNG/JPEG/WebP",
            "media",
            "images",
            "Decodes an encoded image (PNG, JPEG, WebP, GIF) into a raw RGBA pixel stream.",
            "image",
            "purple-500",
            vec![
                inport("input", "Image", "bytes"),
                outport("stream", "RGBA Stream", "stream"),
                outport("error", "Error", "string"),
            ],
            None,
            v,
            c,
        ),
        tpl(
            "tpl_image_encode",
            "Image Encode",
            "Encode to PNG/JPEG/WebP",
            "media",
            "images",
            "Encodes a raw RGBA stream into PNG, JPEG, or WebP format.",
            "file-image",
            "purple-500",
            vec![
                inport("stream", "RGBA Stream", "stream"),
                outport("output", "Encoded", "bytes"),
                outport("metadata", "Metadata", "object"),
                outport("error", "Error", "string"),
            ],
            props(vec![
                (
                    "format",
                    select_prop(
                        "Format",
                        &["png", "jpeg", "webp"],
                        "png",
                        "Output image format",
                    ),
                ),
                (
                    "quality",
                    num_prop("Quality", 90.0, 1.0, 100.0, "JPEG/WebP quality (1-100)"),
                ),
            ]),
            v,
            c,
        ),
    ];

    // Attach display components to the templates that have visual UI
    let display_map: HashMap<&str, DisplayComponent> = HashMap::from([
        // Spectrum analyzer — frequency bar chart with peak hold
        (
            "tpl_audio_spectrum",
            display_inline("reflow-spectrum", SPECTRUM_JS, &["fftSize"], Some("360px")),
        ),
        // Compressor, Limiter, Noise Gate, De-Esser — gain reduction + transfer curve
        (
            "tpl_compressor",
            display_inline(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["thresholdDb", "ratio", "kneeDb"],
                None,
            ),
        ),
        (
            "tpl_limiter",
            display_inline("reflow-dynamics", DYNAMICS_JS, &["ceilingDb"], None),
        ),
        (
            "tpl_noise_gate",
            display_inline(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["thresholdDb", "ratio"],
                None,
            ),
        ),
        (
            "tpl_de_esser",
            display_inline(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["thresholdDb", "ratio"],
                None,
            ),
        ),
        // Equalizer — interactive frequency response curve
        (
            "tpl_equalizer",
            display_inline("reflow-eq", EQ_JS, &["bands", "sampleRate"], Some("360px")),
        ),
        // Audio gain — VU meter with editable gain
        (
            "tpl_audio_gain",
            display_inline("reflow-gain", GAIN_JS, &["gainDb", "gainLinear"], None),
        ),
        // Biquad filter — frequency response curve with editable cutoff
        (
            "tpl_biquad_filter",
            display_inline(
                "reflow-filter-response",
                FILTER_RESPONSE_JS,
                &["filterType", "frequency", "q", "gainDb", "sampleRate"],
                Some("300px"),
            ),
        ),
        // Stream buffer — fill gauge with editable buffer size
        (
            "tpl_stream_buffer",
            display_inline("reflow-buffer", BUFFER_JS, &["bufferBytes"], None),
        ),
        // Image processing actors — live preview
        (
            "tpl_grayscale_filter",
            display_inline("reflow-image-preview", IMAGE_PREVIEW_JS, &[], None),
        ),
        (
            "tpl_brightness_contrast",
            display_inline(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["brightness", "contrast", "saturation"],
                None,
            ),
        ),
        (
            "tpl_chroma_key",
            display_inline(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["keyColor", "tolerance"],
                None,
            ),
        ),
        (
            "tpl_image_resize",
            display_inline(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["width", "height"],
                None,
            ),
        ),
        // Waveform-based actors — scrolling waveform
        (
            "tpl_envelope_follower",
            display_inline(
                "reflow-waveform",
                WAVEFORM_JS,
                &["attackMs", "releaseMs"],
                None,
            ),
        ),
        (
            "tpl_silence_detect",
            display_inline("reflow-waveform", WAVEFORM_JS, &["thresholdDb"], None),
        ),
        (
            "tpl_peak_detect",
            display_inline("reflow-waveform", WAVEFORM_JS, &["sensitivity"], None),
        ),
        // Convolution — IR waveform display
        (
            "tpl_convolve",
            display_inline("reflow-ir", IR_JS, &[], None),
        ),
        // Texture actors — preview thumbnail.
        // observedProps must include output metadata fields (thumbnail, textureWidth,
        // textureHeight, mapping) so Zeal forwards them to the display component
        // when the EventBridge pushes node.output via update_node_properties().
        (
            "tpl_triplanar_texture",
            display_inline(
                "reflow-texture-preview",
                TEXTURE_PREVIEW_JS,
                &[
                    "scale",
                    "sharpness",
                    "thumbnail",
                    "textureWidth",
                    "textureHeight",
                    "mapping",
                ],
                None,
            ),
        ),
        (
            "tpl_uv_texture",
            display_inline(
                "reflow-texture-preview",
                TEXTURE_PREVIEW_JS,
                &[
                    "stride",
                    "uvOffset",
                    "thumbnail",
                    "textureWidth",
                    "textureHeight",
                    "mapping",
                ],
                None,
            ),
        ),
        // Crossover — 3-band frequency response with draggable points
        (
            "tpl_crossover",
            display_inline(
                "reflow-crossover",
                CROSSOVER_JS,
                &["lowFrequency", "highFrequency"],
                Some("320px"),
            ),
        ),
    ]);

    // Apply display components to matching templates
    for template in &mut templates {
        if let Some(dc) = display_map.get(template.id.as_str()) {
            template.display = Some(dc.clone());
        }
    }

    templates
}
