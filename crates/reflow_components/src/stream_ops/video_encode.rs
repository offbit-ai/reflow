//! Video encoder actor — consumes RGBA frame stream, outputs H.264 MP4.
//!
//! Backend per target:
//! - **Native**: openh264 (pure Rust H.264 encoder, no system FFmpeg).
//! - **wasm32**: WebCodecs `VideoEncoder` via web-sys (Chromium /
//!   Edge / Safari ship hardware acceleration; Firefox-on-Android
//!   does not, see <https://developer.mozilla.org/en-US/docs/Web/API/VideoEncoder>).
//!
//! Both paths produce AVCC-framed H.264 NALs that the shared
//! `mux_mp4` ISOBMFF muxer wraps into an MP4 — the on-the-wire
//! output is identical.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
use openh264::encoder::{Encoder, EncoderConfig};
#[cfg(not(target_arch = "wasm32"))]
use openh264::formats::{RgbSliceU8, YUVBuffer};
use reflow_actor::{message::EncodableValue, stream::StreamFrame, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    VideoEncoderActor,
    inports::<100>(stream),
    outports::<50>(output, metadata, error),
    state(MemoryState)
)]
pub async fn video_encoder_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let target_fps = config.get("fps").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

    // Take stream receiver — only proceed if we actually got a StreamHandle
    let rx = match ctx.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(HashMap::new()),
    };

    // Bitrate in kbps from config (default 5000 = 5 Mbps for crisp output)
    let bitrate_kbps = config
        .get("bitrate")
        .and_then(|v| v.as_u64())
        .unwrap_or(5000) as u32;

    let tfps = target_fps;

    // Stream-encode: encode each frame as it arrives, keeping only
    // compressed NALs. No raw RGBA frames are accumulated — memory
    // stays constant.
    //
    // Native: openh264 is sync + CPU-heavy, so we hand it to
    // `spawn_blocking` to keep the tokio reactor responsive. Wasm:
    // WebCodecs is event-driven and the runtime is single-threaded
    // (`spawn_local`), so we just await the encoder pipeline directly.
    #[cfg(not(target_arch = "wasm32"))]
    let (mp4_bytes, width, height, fps, frame_count) =
        tokio::task::spawn_blocking(move || stream_encode(rx, tfps, bitrate_kbps))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn failed: {}", e))?
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    #[cfg(target_arch = "wasm32")]
    let (mp4_bytes, width, height, fps, frame_count) = stream_encode_wasm(rx, tfps, bitrate_kbps)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::bytes(mp4_bytes.clone()));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "format": "mp4",
            "codec": "h264",
            "width": width,
            "height": height,
            "fps": fps,
            "frameCount": frame_count,
            "fileSize": mp4_bytes.len(),
        }))),
    );
    Ok(out)
}

/// Stream-encode: process each RGBA frame as it arrives from the stream,
/// encode to H.264 immediately, and keep only the compressed NALs.
/// Raw RGBA frames are never accumulated — memory stays constant.
#[cfg(not(target_arch = "wasm32"))]
fn stream_encode(
    rx: flume::Receiver<StreamFrame>,
    target_fps: u32,
    bitrate_kbps: u32,
) -> Result<(Vec<u8>, u32, u32, u32, usize), String> {
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = target_fps;
    let mut encoder: Option<Encoder> = None;
    let mut nal_units: Vec<Vec<u8>> = Vec::new();
    let mut avcc_sizes: Vec<u32> = Vec::new();

    loop {
        match rx.recv() {
            Ok(StreamFrame::Begin { metadata, .. }) => {
                if let Some(md) = metadata {
                    width = md.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
                    height = md.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
                    fps = md
                        .get("fps")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(target_fps as u64) as u32;
                }
            }
            Ok(StreamFrame::Data(rgba)) => {
                // Lazily init encoder on first data frame (need width/height)
                if encoder.is_none() && width > 0 && height > 0 {
                    let config = EncoderConfig::new()
                        .set_bitrate_bps(bitrate_kbps * 1000)
                        .max_frame_rate(fps as f32)
                        .rate_control_mode(openh264::encoder::RateControlMode::Off);
                    let api = openh264::OpenH264API::from_source();
                    encoder = Some(
                        Encoder::with_api_config(api, config)
                            .map_err(|e| format!("Encoder init: {}", e))?,
                    );
                }

                if let Some(ref mut enc) = encoder {
                    let rgb = rgba_to_rgb(&rgba, width, height);
                    let rgb_source = RgbSliceU8::new(&rgb, (width as usize, height as usize));
                    let yuv = YUVBuffer::from_rgb_source(rgb_source);
                    let bitstream = enc.encode(&yuv).map_err(|e| format!("Encode: {}", e))?;

                    let mut frame_data = Vec::new();
                    for layer_idx in 0..bitstream.num_layers() {
                        if let Some(layer) = bitstream.layer(layer_idx) {
                            for nal_idx in 0..layer.nal_count() {
                                if let Some(nal) = layer.nal_unit(nal_idx) {
                                    frame_data.extend_from_slice(nal);
                                }
                            }
                        }
                    }

                    let avcc = annex_b_to_avcc(&frame_data);
                    avcc_sizes.push(avcc.len() as u32);
                    nal_units.push(avcc);
                }
                // rgba is dropped here — no accumulation
            }
            Ok(StreamFrame::End) => break,
            Ok(StreamFrame::Error(e)) => return Err(e),
            Err(_) => break,
        }
    }

    let frame_count = nal_units.len();
    if frame_count == 0 {
        return Err("No frames received".to_string());
    }

    let mp4 = mux_mp4(&nal_units, &avcc_sizes, width, height, fps);
    Ok((mp4, width, height, fps, frame_count))
}

// ─── wasm32: WebCodecs VideoEncoder ────────────────────────────────────────

/// Stream-encode via the browser's `VideoEncoder` (WebCodecs).
///
/// Mirrors the openh264 path: read `Begin` for resolution/fps,
/// encode each `Data` frame as it arrives, accumulate AVCC NALs,
/// flush on `End`, then mux to MP4. The flush/encode cycle is
/// driven by Promises that we await via wasm-bindgen-futures.
///
/// The `output` callback fires asynchronously as the encoder
/// produces chunks; we collect them into a shared `RefCell<Vec<…>>`
/// (sound under wasm32's single-thread invariant).
#[cfg(target_arch = "wasm32")]
async fn stream_encode_wasm(
    rx: flume::Receiver<StreamFrame>,
    target_fps: u32,
    bitrate_kbps: u32,
) -> Result<(Vec<u8>, u32, u32, u32, usize), String> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        EncodedVideoChunk, VideoEncoder, VideoEncoderConfig, VideoEncoderInit, VideoFrame,
        VideoFrameBufferInit, VideoPixelFormat,
    };

    // Buffer encoder output. WebCodecs invokes our output callback
    // asynchronously; the runtime is single-threaded so a `Rc<RefCell<…>>`
    // is sound here.
    let nal_units: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let avcc_sizes: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let encoder_error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let nals_for_cb = nal_units.clone();
    let sizes_for_cb = avcc_sizes.clone();
    let output_cb = Closure::<dyn FnMut(EncodedVideoChunk, JsValue)>::new(
        move |chunk: EncodedVideoChunk, _meta: JsValue| {
            let len = chunk.byte_length() as usize;
            let mut buf = vec![0u8; len];
            if chunk.copy_to_with_u8_slice(&mut buf).is_err() {
                return;
            }
            // WebCodecs emits Annex-B by default; convert to AVCC
            // to match the openh264 path's framing.
            let avcc = annex_b_to_avcc(&buf);
            sizes_for_cb.borrow_mut().push(avcc.len() as u32);
            nals_for_cb.borrow_mut().push(avcc);
        },
    );

    let err_cell = encoder_error.clone();
    let error_cb = Closure::<dyn FnMut(JsValue)>::new(move |err: JsValue| {
        let msg = err
            .dyn_ref::<js_sys::Error>()
            .map(|e| e.message().as_string().unwrap_or_else(|| format!("{:?}", err)))
            .unwrap_or_else(|| format!("{:?}", err));
        *err_cell.borrow_mut() = Some(msg);
    });

    let init = VideoEncoderInit::new(
        error_cb.as_ref().unchecked_ref(),
        output_cb.as_ref().unchecked_ref(),
    );
    let encoder = VideoEncoder::new(&init).map_err(|e| format!("VideoEncoder::new: {:?}", e))?;
    // The closures must outlive every encoder callback. forget()
    // leaks them deliberately — the encoder is dropped at end of
    // function and the runtime cleans up on actor shutdown.
    output_cb.forget();
    error_cb.forget();

    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = target_fps;
    let mut configured = false;
    let mut timestamp_us: f64 = 0.0;

    loop {
        // Yield to the event loop so encoder callbacks can fire
        // between frames. flume's recv_async is the wasm-friendly
        // sibling of recv().
        let frame = match rx.recv_async().await {
            Ok(f) => f,
            Err(_) => break,
        };
        match frame {
            StreamFrame::Begin { metadata, .. } => {
                if let Some(md) = metadata {
                    width = md.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
                    height = md.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
                    fps = md
                        .get("fps")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(target_fps as u64) as u32;
                }
            }
            StreamFrame::Data(rgba) => {
                if !configured && width > 0 && height > 0 {
                    // avc1.42E01F = H.264 Baseline profile, level 3.1.
                    // Widely supported across browsers / hardware
                    // decoders; matches what openh264 emits with
                    // default settings.
                    let config = VideoEncoderConfig::new("avc1.42E01F", width, height);
                    config.set_bitrate(bitrate_kbps as f64 * 1000.0);
                    config.set_framerate(fps as f64);
                    encoder
                        .configure(&config)
                        .map_err(|e| format!("configure: {:?}", e))?;
                    configured = true;
                }
                if configured {
                    // web-sys signature: (coded_height, coded_width,
                    // format, timestamp_microseconds).
                    let buffer_init =
                        VideoFrameBufferInit::new(height, width, VideoPixelFormat::Rgba, timestamp_us);
                    // SAFETY: `as_slice()` returns a `&[u8]` view of
                    // the Rc<Vec<u8>>; the JS side copies into its
                    // own buffer before returning, so the lifetime
                    // is fine.
                    let array = js_sys::Uint8Array::from(rgba.as_ref().as_slice());
                    let frame = VideoFrame::new_with_buffer_source_and_video_frame_buffer_init(
                        &array.into(),
                        &buffer_init,
                    )
                    .map_err(|e| format!("VideoFrame::new: {:?}", e))?;

                    encoder
                        .encode(&frame)
                        .map_err(|e| format!("encode: {:?}", e))?;
                    frame.close();
                    timestamp_us += 1_000_000.0 / fps as f64;
                }
            }
            StreamFrame::End => break,
            StreamFrame::Error(e) => return Err(e),
        }

        if let Some(err) = encoder_error.borrow_mut().take() {
            return Err(err);
        }
    }

    // Drain the encoder so any in-flight chunks make it to our
    // output callback before we collect them.
    JsFuture::from(encoder.flush())
        .await
        .map_err(|e| format!("flush: {:?}", e))?;
    encoder.close();

    if let Some(err) = encoder_error.borrow_mut().take() {
        return Err(err);
    }

    // Drop the Rc layer — at this point only `nal_units` is alive
    // (the closure was forgotten and dropped its ref).
    let nals = Rc::try_unwrap(nal_units)
        .map_err(|_| "encoder still holds nal_units ref".to_string())?
        .into_inner();
    let sizes = Rc::try_unwrap(avcc_sizes)
        .map_err(|_| "encoder still holds avcc_sizes ref".to_string())?
        .into_inner();

    let frame_count = nals.len();
    if frame_count == 0 {
        return Err("No frames received".to_string());
    }

    let mp4 = mux_mp4(&nals, &sizes, width, height, fps);
    Ok((mp4, width, height, fps, frame_count))
}

/// RGBA→RGB conversion. Processes 4 pixels at a time for auto-vectorization.
fn rgba_to_rgb(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut rgb = vec![0u8; pixel_count * 3];

    // Process 4 pixels at a time (16 RGBA bytes → 12 RGB bytes)
    let chunks = pixel_count / 4;
    for c in 0..chunks {
        let si = c * 16; // source: 4 pixels × 4 bytes
        let di = c * 12; // dest: 4 pixels × 3 bytes
                         // Batch copy — LLVM vectorizes this into SIMD shuffle
        rgb[di] = rgba[si];
        rgb[di + 1] = rgba[si + 1];
        rgb[di + 2] = rgba[si + 2];
        rgb[di + 3] = rgba[si + 4];
        rgb[di + 4] = rgba[si + 5];
        rgb[di + 5] = rgba[si + 6];
        rgb[di + 6] = rgba[si + 8];
        rgb[di + 7] = rgba[si + 9];
        rgb[di + 8] = rgba[si + 10];
        rgb[di + 9] = rgba[si + 12];
        rgb[di + 10] = rgba[si + 13];
        rgb[di + 11] = rgba[si + 14];
    }
    // Remainder
    for i in (chunks * 4)..pixel_count {
        let si = i * 4;
        let di = i * 3;
        if si + 2 < rgba.len() {
            rgb[di] = rgba[si];
            rgb[di + 1] = rgba[si + 1];
            rgb[di + 2] = rgba[si + 2];
        }
    }
    rgb
}

fn annex_b_to_avcc(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        // Find start code (00 00 00 01 or 00 00 01)
        let start = if i + 4 <= data.len() && data[i..i + 4] == [0, 0, 0, 1] {
            i + 4
        } else if i + 3 <= data.len() && data[i..i + 3] == [0, 0, 1] {
            i + 3
        } else {
            i += 1;
            continue;
        };

        // Find next start code or end
        let mut end = data.len();
        for j in start..data.len().saturating_sub(3) {
            if (j + 4 <= data.len() && data[j..j + 4] == [0, 0, 0, 1])
                || data[j..j + 3] == [0, 0, 1]
            {
                end = j;
                break;
            }
        }

        let nal = &data[start..end];
        let len = nal.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(nal);
        i = end;
    }

    if out.is_empty() && !data.is_empty() {
        let len = data.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(data);
    }

    out
}

#[allow(dead_code)]
fn extract_sps_pps(first_frame: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    let mut i = 0;

    while i < first_frame.len() {
        let start = if i + 4 <= first_frame.len() && first_frame[i..i + 4] == [0, 0, 0, 1] {
            i + 4
        } else if i + 3 <= first_frame.len() && first_frame[i..i + 3] == [0, 0, 1] {
            i + 3
        } else {
            i += 1;
            continue;
        };

        let mut end = first_frame.len();
        for j in start..first_frame.len().saturating_sub(3) {
            if (j + 4 <= first_frame.len() && first_frame[j..j + 4] == [0, 0, 0, 1])
                || first_frame[j..j + 3] == [0, 0, 1]
            {
                end = j;
                break;
            }
        }

        if start < first_frame.len() {
            let nal_type = first_frame[start] & 0x1f;
            let nal_data = &first_frame[start..end];
            match nal_type {
                7 => sps = nal_data.to_vec(),
                8 => pps = nal_data.to_vec(),
                _ => {}
            }
        }
        i = end;
    }

    if sps.is_empty() {
        sps = vec![0x67, 0x42, 0x00, 0x0a, 0xf8, 0x41, 0xa2];
    }
    if pps.is_empty() {
        pps = vec![0x68, 0xce, 0x38, 0x80];
    }
    (sps, pps)
}

// ─── Minimal MP4 Muxer ──────────────────────────────────────────────

fn mux_mp4(
    nal_units: &[Vec<u8>],
    avcc_sizes: &[u32],
    width: u32,
    height: u32,
    fps: u32,
) -> Vec<u8> {
    let timescale = 90000u32;
    let frame_duration = timescale / fps;
    let total_duration = frame_duration * nal_units.len() as u32;

    // Concatenate all AVCC frame data for mdat
    let mut mdat_payload = Vec::new();
    for data in nal_units {
        mdat_payload.extend_from_slice(data);
    }

    // Extract SPS/PPS from first original frame (before AVCC conversion)
    // We need to re-extract from the AVCC data — just use fallback SPS/PPS for now
    // since openh264 emits them in the first encoded frame's NAL layers
    let (sps, pps) = if !nal_units.is_empty() {
        // AVCC format: [4-byte length][NAL data]...
        // Parse first NALs to find SPS (type 7) and PPS (type 8)
        extract_sps_pps_from_avcc(&nal_units[0])
    } else {
        (vec![0x67, 0x42, 0x00, 0x0a], vec![0x68, 0xce, 0x38, 0x80])
    };

    // Build ftyp + moov (with placeholder offset), measure, then rebuild with correct offset
    let ftyp = build_ftyp();
    let moov_placeholder = build_moov(
        width,
        height,
        timescale,
        total_duration,
        frame_duration,
        avcc_sizes,
        &sps,
        &pps,
        0,
    );
    let mdat_header_size = 8u32;
    let mdat_offset = ftyp.len() as u32 + moov_placeholder.len() as u32 + mdat_header_size;

    let moov = build_moov(
        width,
        height,
        timescale,
        total_duration,
        frame_duration,
        avcc_sizes,
        &sps,
        &pps,
        mdat_offset,
    );

    let mdat_size = mdat_header_size + mdat_payload.len() as u32;
    let mut mdat = Vec::new();
    mdat.extend_from_slice(&mdat_size.to_be_bytes());
    mdat.extend_from_slice(b"mdat");
    mdat.extend_from_slice(&mdat_payload);

    let mut mp4 = Vec::with_capacity(ftyp.len() + moov.len() + mdat.len());
    mp4.extend_from_slice(&ftyp);
    mp4.extend_from_slice(&moov);
    mp4.extend_from_slice(&mdat);
    mp4
}

fn extract_sps_pps_from_avcc(avcc_data: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    let mut i = 0;
    while i + 4 < avcc_data.len() {
        let len = u32::from_be_bytes(avcc_data[i..i + 4].try_into().unwrap()) as usize;
        let start = i + 4;
        if start < avcc_data.len() {
            let nal_type = avcc_data[start] & 0x1f;
            let end = (start + len).min(avcc_data.len());
            match nal_type {
                7 => sps = avcc_data[start..end].to_vec(),
                8 => pps = avcc_data[start..end].to_vec(),
                _ => {}
            }
            i = end;
        } else {
            break;
        }
    }
    if sps.is_empty() {
        sps = vec![0x67, 0x42, 0x00, 0x0a, 0xf8, 0x41, 0xa2];
    }
    if pps.is_empty() {
        pps = vec![0x68, 0xce, 0x38, 0x80];
    }
    (sps, pps)
}

fn build_ftyp() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"isom");
    b.extend_from_slice(&0x200u32.to_be_bytes());
    b.extend_from_slice(b"isomiso2avc1mp41");
    mp4_box(b"ftyp", &b)
}

fn build_moov(
    w: u32,
    h: u32,
    ts: u32,
    dur: u32,
    fd: u32,
    sizes: &[u32],
    sps: &[u8],
    pps: &[u8],
    mdat_off: u32,
) -> Vec<u8> {
    let mvhd = build_mvhd(ts, dur);
    let trak = build_trak(w, h, ts, dur, fd, sizes, sps, pps, mdat_off);
    let mut content = mvhd;
    content.extend_from_slice(&trak);
    mp4_box(b"moov", &content)
}

fn build_mvhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0u8; 4]); // version + flags
    b.extend_from_slice(&0u32.to_be_bytes()); // creation
    b.extend_from_slice(&0u32.to_be_bytes()); // modification
    b.extend_from_slice(&timescale.to_be_bytes());
    b.extend_from_slice(&duration.to_be_bytes());
    b.extend_from_slice(&0x00010000u32.to_be_bytes()); // rate
    b.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
    b.extend_from_slice(&[0u8; 10]); // reserved
    for &val in &[0x00010000u32, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
        b.extend_from_slice(&val.to_be_bytes());
    }
    b.extend_from_slice(&[0u8; 24]); // pre_defined
    b.extend_from_slice(&2u32.to_be_bytes()); // next_track_ID
    mp4_box(b"mvhd", &b)
}

fn build_trak(
    w: u32,
    h: u32,
    ts: u32,
    dur: u32,
    fd: u32,
    sizes: &[u32],
    sps: &[u8],
    pps: &[u8],
    mdat_off: u32,
) -> Vec<u8> {
    let tkhd = build_tkhd(w, h, dur);
    let mdia = build_mdia(w, h, ts, dur, fd, sizes, sps, pps, mdat_off);
    let mut content = tkhd;
    content.extend_from_slice(&mdia);
    mp4_box(b"trak", &content)
}

fn build_tkhd(width: u32, height: u32, duration: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0, 0, 0, 3]); // version=0, flags=3
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&1u32.to_be_bytes()); // track_ID
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&duration.to_be_bytes());
    b.extend_from_slice(&[0u8; 8]);
    b.extend_from_slice(&0u16.to_be_bytes()); // layer
    b.extend_from_slice(&0u16.to_be_bytes()); // alternate_group
    b.extend_from_slice(&0u16.to_be_bytes()); // volume
    b.extend_from_slice(&0u16.to_be_bytes());
    for &val in &[0x00010000u32, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
        b.extend_from_slice(&val.to_be_bytes());
    }
    b.extend_from_slice(&(width << 16).to_be_bytes());
    b.extend_from_slice(&(height << 16).to_be_bytes());
    mp4_box(b"tkhd", &b)
}

fn build_mdia(
    w: u32,
    h: u32,
    ts: u32,
    dur: u32,
    fd: u32,
    sizes: &[u32],
    sps: &[u8],
    pps: &[u8],
    mdat_off: u32,
) -> Vec<u8> {
    let mdhd = build_mdhd(ts, dur);
    let hdlr = build_hdlr();
    let minf = build_minf(w, h, fd, sizes, sps, pps, mdat_off);
    let mut content = mdhd;
    content.extend_from_slice(&hdlr);
    content.extend_from_slice(&minf);
    mp4_box(b"mdia", &content)
}

fn build_mdhd(timescale: u32, duration: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0u8; 4]);
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&timescale.to_be_bytes());
    b.extend_from_slice(&duration.to_be_bytes());
    b.extend_from_slice(&0x55c4u16.to_be_bytes()); // und
    b.extend_from_slice(&0u16.to_be_bytes());
    mp4_box(b"mdhd", &b)
}

fn build_hdlr() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0u8; 4]);
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(b"vide");
    b.extend_from_slice(&[0u8; 12]);
    b.extend_from_slice(b"VideoHandler\0");
    mp4_box(b"hdlr", &b)
}

fn build_minf(
    w: u32,
    h: u32,
    fd: u32,
    sizes: &[u32],
    sps: &[u8],
    pps: &[u8],
    mdat_off: u32,
) -> Vec<u8> {
    let vmhd = build_vmhd();
    let dinf = build_dinf();
    let stbl = build_stbl(w, h, fd, sizes, sps, pps, mdat_off);
    let mut content = vmhd;
    content.extend_from_slice(&dinf);
    content.extend_from_slice(&stbl);
    mp4_box(b"minf", &content)
}

fn build_vmhd() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 1];
    b.extend_from_slice(&0u16.to_be_bytes());
    b.extend_from_slice(&[0u8; 6]);
    mp4_box(b"vmhd", &b)
}

fn build_dinf() -> Vec<u8> {
    // url box (self-contained)
    let url = vec![0, 0, 0, 1]; // version=0, flags=1 (self-contained)
    let url_box = mp4_box(b"url ", &url);

    let mut dref = vec![0u8; 4]; // version + flags
    dref.extend_from_slice(&1u32.to_be_bytes());
    dref.extend_from_slice(&url_box);
    let dref_box = mp4_box(b"dref", &dref);

    mp4_box(b"dinf", &dref_box)
}

fn build_stbl(
    w: u32,
    h: u32,
    fd: u32,
    sizes: &[u32],
    sps: &[u8],
    pps: &[u8],
    mdat_off: u32,
) -> Vec<u8> {
    let stsd = build_stsd(w, h, sps, pps);
    let stts = build_stts(sizes.len() as u32, fd);
    let stsz = build_stsz(sizes);
    let stsc = build_stsc(sizes.len() as u32);
    let stco = build_stco(mdat_off);

    let mut content = stsd;
    content.extend_from_slice(&stts);
    content.extend_from_slice(&stsz);
    content.extend_from_slice(&stsc);
    content.extend_from_slice(&stco);
    mp4_box(b"stbl", &content)
}

fn build_stsd(width: u32, height: u32, sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let avcc = build_avcc(sps, pps);
    let mut avc1 = Vec::new();
    avc1.extend_from_slice(&[0u8; 6]); // reserved
    avc1.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
    avc1.extend_from_slice(&[0u8; 16]); // pre_defined + reserved
    avc1.extend_from_slice(&(width as u16).to_be_bytes());
    avc1.extend_from_slice(&(height as u16).to_be_bytes());
    avc1.extend_from_slice(&0x00480000u32.to_be_bytes()); // horiz resolution
    avc1.extend_from_slice(&0x00480000u32.to_be_bytes()); // vert resolution
    avc1.extend_from_slice(&0u32.to_be_bytes());
    avc1.extend_from_slice(&1u16.to_be_bytes()); // frame_count
    avc1.extend_from_slice(&[0u8; 32]); // compressorname
    avc1.extend_from_slice(&0x0018u16.to_be_bytes()); // depth
    avc1.extend_from_slice(&0xffffu16.to_be_bytes()); // pre_defined (-1)
    avc1.extend_from_slice(&avcc);
    let avc1_box = mp4_box(b"avc1", &avc1);

    let mut b = vec![0u8; 4]; // version + flags
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(&avc1_box);
    mp4_box(b"stsd", &b)
}

fn build_avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.push(1);
    b.push(if sps.len() > 1 { sps[1] } else { 66 });
    b.push(if sps.len() > 2 { sps[2] } else { 0 });
    b.push(if sps.len() > 3 { sps[3] } else { 10 });
    b.push(0xff); // lengthSizeMinusOne = 3
    b.push(0xe1); // numSPS = 1
    b.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    b.extend_from_slice(sps);
    b.push(1); // numPPS = 1
    b.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    b.extend_from_slice(pps);
    mp4_box(b"avcC", &b)
}

fn build_stts(count: u32, delta: u32) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(&count.to_be_bytes());
    b.extend_from_slice(&delta.to_be_bytes());
    mp4_box(b"stts", &b)
}

fn build_stsz(sizes: &[u32]) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(&0u32.to_be_bytes()); // sample_size=0 (variable)
    b.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
    for &s in sizes {
        b.extend_from_slice(&s.to_be_bytes());
    }
    mp4_box(b"stsz", &b)
}

fn build_stsc(count: u32) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(&1u32.to_be_bytes()); // first_chunk
    b.extend_from_slice(&count.to_be_bytes()); // samples_per_chunk
    b.extend_from_slice(&1u32.to_be_bytes()); // sample_desc_index
    mp4_box(b"stsc", &b)
}

fn build_stco(mdat_offset: u32) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(&mdat_offset.to_be_bytes());
    mp4_box(b"stco", &b)
}

fn mp4_box(box_type: &[u8; 4], content: &[u8]) -> Vec<u8> {
    let size = (8 + content.len()) as u32;
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(&size.to_be_bytes());
    b.extend_from_slice(box_type);
    b.extend_from_slice(content);
    b
}

fn error_out(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}
