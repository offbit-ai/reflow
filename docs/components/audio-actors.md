# Audio / DSP Actors

Actors behind the `av-core` feature of `reflow_components` / `reflow_rt`. Enabling `av-core` pulls in `reflow_dsp` and activates the audio stream-ops catalog.

```toml
reflow_rt = { version = "0.1", features = ["av-core"] }
```

## What's included

| Category | Templates |
|----------|-----------|
| Dynamics | `tpl_compressor`, `tpl_limiter`, `tpl_noise_gate`, `tpl_de_esser` |
| Filters | `tpl_biquad_filter`, `tpl_equalizer`, `tpl_crossover`, `tpl_convolve` |
| Gain / normalization | `tpl_audio_gain`, `tpl_audio_normalize`, `tpl_dc_offset` |
| Spectral | `tpl_audio_spectrum`, `tpl_ifft` |
| Dynamics detection | `tpl_envelope_follower`, `tpl_peak_detect`, `tpl_silence_detect` |
| Time / pitch | `tpl_time_stretch`, `tpl_pitch_shift` |
| Correlation / reduction | `tpl_correlator`, `tpl_noise_reduction` |
| Display | `tpl_audio_stream_display` |

All actors consume and produce audio streams via `Message::StreamHandle` so large buffers never cross the message bus directly.

## Complete per-template catalog

See **[standard-library.md § Stream Ops / Audio sections](./standard-library.md)**.

## Related

- [Media actors](./media-actors.md) — `tpl_audio_input` belongs here for URL / binary resolution before the DSP chain.
