//! Display metadata colocated with stream operation actors.

use reflow_network::template::{DisplayComponentSource, NodeTemplate};

const SPECTRUM_JS: &str = include_str!("display/spectrum.js");
const DYNAMICS_JS: &str = include_str!("display/dynamics.js");
const EQ_JS: &str = include_str!("display/eq.js");
const STATS_JS: &str = include_str!("display/stats.js");
const CROSSOVER_JS: &str = include_str!("display/crossover.js");
const GAIN_JS: &str = include_str!("display/gain.js");
const FILTER_RESPONSE_JS: &str = include_str!("display/filter_response.js");
const BUFFER_JS: &str = include_str!("display/buffer.js");
const IMAGE_PREVIEW_JS: &str = include_str!("display/image_preview.js");
const WAVEFORM_JS: &str = include_str!("display/waveform.js");
const IR_JS: &str = include_str!("display/ir.js");

pub(crate) fn display_component_sources() -> Vec<(&'static str, &'static str)> {
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
    ]
}

pub(crate) fn display_catalog_entries() -> Vec<DisplayComponentSource> {
    display_component_sources()
        .into_iter()
        .map(|(element, source)| DisplayComponentSource {
            element: element.to_string(),
            source: source.to_string(),
        })
        .collect()
}

pub(crate) fn attach_display_components(templates: &mut [NodeTemplate]) {
    for template in templates {
        template.display = match template.id.as_str() {
            "tpl_audio_spectrum" => Some(crate::display::inline_display(
                "reflow-spectrum",
                SPECTRUM_JS,
                &["fftSize"],
                Some("360px"),
            )),
            "tpl_compressor" => Some(crate::display::inline_display(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["thresholdDb", "ratio", "kneeDb"],
                None,
            )),
            "tpl_limiter" => Some(crate::display::inline_display(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["ceilingDb"],
                None,
            )),
            "tpl_noise_gate" | "tpl_de_esser" => Some(crate::display::inline_display(
                "reflow-dynamics",
                DYNAMICS_JS,
                &["thresholdDb", "ratio"],
                None,
            )),
            "tpl_equalizer" => Some(crate::display::inline_display(
                "reflow-eq",
                EQ_JS,
                &["bands", "sampleRate"],
                Some("360px"),
            )),
            "tpl_audio_gain" => Some(crate::display::inline_display(
                "reflow-gain",
                GAIN_JS,
                &["gainDb", "gainLinear"],
                None,
            )),
            "tpl_biquad_filter" => Some(crate::display::inline_display(
                "reflow-filter-response",
                FILTER_RESPONSE_JS,
                &["filterType", "frequency", "q", "gainDb", "sampleRate"],
                Some("300px"),
            )),
            "tpl_stream_buffer" => Some(crate::display::inline_display(
                "reflow-buffer",
                BUFFER_JS,
                &["bufferBytes"],
                None,
            )),
            "tpl_grayscale_filter" => Some(crate::display::inline_display(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &[],
                None,
            )),
            "tpl_brightness_contrast" => Some(crate::display::inline_display(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["brightness", "contrast", "saturation"],
                None,
            )),
            "tpl_chroma_key" => Some(crate::display::inline_display(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["keyColor", "tolerance"],
                None,
            )),
            "tpl_image_resize" => Some(crate::display::inline_display(
                "reflow-image-preview",
                IMAGE_PREVIEW_JS,
                &["width", "height"],
                None,
            )),
            "tpl_envelope_follower" => Some(crate::display::inline_display(
                "reflow-waveform",
                WAVEFORM_JS,
                &["attackMs", "releaseMs"],
                None,
            )),
            "tpl_silence_detect" => Some(crate::display::inline_display(
                "reflow-waveform",
                WAVEFORM_JS,
                &["thresholdDb"],
                None,
            )),
            "tpl_peak_detect" => Some(crate::display::inline_display(
                "reflow-waveform",
                WAVEFORM_JS,
                &["sensitivity"],
                None,
            )),
            "tpl_convolve" => Some(crate::display::inline_display(
                "reflow-ir",
                IR_JS,
                &[],
                None,
            )),
            "tpl_crossover" => Some(crate::display::inline_display(
                "reflow-crossover",
                CROSSOVER_JS,
                &["lowFrequency", "highFrequency"],
                Some("320px"),
            )),
            _ => template.display.take(),
        };
    }
}
