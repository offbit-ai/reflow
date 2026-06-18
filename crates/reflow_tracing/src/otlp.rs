//! OTLP export bridge.
//!
//! Converts a finalized [`FlowTrace`] into OpenTelemetry spans and ships them to
//! any OTLP/HTTP collector (Monoscope, Jaeger, Tempo, Grafana, Honeycomb, …) via
//! `POST {endpoint}/v1/traces`. We hand-roll the OTLP/JSON payload rather than
//! pulling the full OpenTelemetry SDK: the wire format is documented and stable,
//! and reflow's trace model is already span-shaped, so the mapping is direct.
//!
//! Mapping:
//! - `FlowTrace` → one OTel **trace** (the reflow `trace_id`, a 128-bit UUID, is
//!   used directly as the 16-byte OTel trace id).
//! - a synthetic **root span** spans the flow (`flow:{flow_id}`).
//! - each `TraceEvent` → a **child span**, parented to its
//!   `causality.parent_event_id` when set, else the root. `ActorFailed` sets an
//!   error status; `DataFlow`/message attributes (incl. the content checksum)
//!   become span attributes.
//!
//! Per the OTLP/JSON encoding, trace/span ids are **hex** strings and unix-nano
//! timestamps are decimal **strings**.

use crate::config::OtlpConfig;
use anyhow::Result;
use chrono::{DateTime, Utc};
use reflow_tracing_protocol::{FlowTrace, TraceEvent, TraceEventType};
use serde_json::{json, Value};

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn unix_nanos(t: DateTime<Utc>) -> String {
    t.timestamp_nanos_opt().unwrap_or(0).max(0).to_string()
}

/// `{"key": k, "value": {"stringValue": v}}`
fn attr(key: &str, value: impl Into<String>) -> Value {
    json!({ "key": key, "value": { "stringValue": value.into() } })
}

fn event_type_name(t: &TraceEventType) -> &'static str {
    match t {
        TraceEventType::ActorCreated => "ActorCreated",
        TraceEventType::ActorStarted => "ActorStarted",
        TraceEventType::ActorCompleted => "ActorCompleted",
        TraceEventType::ActorFailed => "ActorFailed",
        TraceEventType::MessageSent => "MessageSent",
        TraceEventType::MessageReceived => "MessageReceived",
        TraceEventType::StateChanged => "StateChanged",
        TraceEventType::PortConnected => "PortConnected",
        TraceEventType::PortDisconnected => "PortDisconnected",
        TraceEventType::DataFlow { .. } => "DataFlow",
        TraceEventType::NetworkEvent => "NetworkEvent",
    }
}

fn span_name(e: &TraceEvent) -> String {
    match &e.event_type {
        TraceEventType::DataFlow { to_actor, .. } => {
            format!("dataflow:{}->{}", e.actor_id, to_actor)
        }
        other => format!("{}:{}", event_type_name(other), e.actor_id),
    }
}

fn build_event_span(trace_id_hex: &str, root_span_id: &str, e: &TraceEvent) -> Value {
    let span_id = to_hex(&e.event_id.0.as_bytes()[..8]);
    let parent = e
        .causality
        .parent_event_id
        .as_ref()
        .map(|p| to_hex(&p.0.as_bytes()[..8]))
        .unwrap_or_else(|| root_span_id.to_string());

    let start = e.timestamp.timestamp_nanos_opt().unwrap_or(0).max(0);
    let dur = e.data.performance_metrics.execution_time_ns.unwrap_or(0) as i64;
    let end = start + dur;

    let mut attrs = vec![
        attr("reflow.actor_id", e.actor_id.clone()),
        attr("reflow.event_type", event_type_name(&e.event_type)),
    ];
    if let Some(port) = &e.data.port {
        attrs.push(attr("reflow.port", port.clone()));
    }
    if let TraceEventType::DataFlow { to_actor, to_port } = &e.event_type {
        attrs.push(attr("reflow.to_actor", to_actor.clone()));
        attrs.push(attr("reflow.to_port", to_port.clone()));
    }
    if let Some(m) = &e.data.message {
        attrs.push(attr("reflow.message.type", m.message_type.clone()));
        attrs.push(attr("reflow.message.size_bytes", m.size_bytes.to_string()));
        if !m.checksum.is_empty() {
            attrs.push(attr("reflow.message.checksum", m.checksum.clone()));
        }
    }

    let mut span = json!({
        "traceId": trace_id_hex,
        "spanId": span_id,
        "parentSpanId": parent,
        "name": span_name(e),
        "kind": 1, // SPAN_KIND_INTERNAL
        "startTimeUnixNano": start.to_string(),
        "endTimeUnixNano": end.to_string(),
        "attributes": attrs,
    });
    if matches!(e.event_type, TraceEventType::ActorFailed) {
        span["status"] = json!({
            "code": 2, // STATUS_CODE_ERROR
            "message": e.data.error.clone().unwrap_or_default(),
        });
    }
    span
}

/// Convert a finalized `FlowTrace` to an OTLP/JSON `ExportTraceServiceRequest`.
pub fn flow_trace_to_otlp_json(trace: &FlowTrace, service_name: &str) -> Value {
    let trace_id_hex = to_hex(trace.trace_id.0.as_bytes());
    let root_span_id = to_hex(&trace.execution_id.0.as_bytes()[..8]);

    let start = trace.start_time.timestamp_nanos_opt().unwrap_or(0).max(0);
    let end = trace
        .end_time
        .map(|t| t.timestamp_nanos_opt().unwrap_or(0).max(0))
        .unwrap_or_else(|| {
            trace
                .events
                .iter()
                .filter_map(|e| e.timestamp.timestamp_nanos_opt())
                .max()
                .unwrap_or(start)
        });

    let mut spans = Vec::with_capacity(trace.events.len() + 1);
    // Root span for the flow.
    spans.push(json!({
        "traceId": trace_id_hex,
        "spanId": root_span_id,
        "name": format!("flow:{}", trace.flow_id.0),
        "kind": 1,
        "startTimeUnixNano": start.to_string(),
        "endTimeUnixNano": end.to_string(),
        "attributes": [
            attr("reflow.flow_id", trace.flow_id.0.clone()),
            attr("reflow.execution_id", trace.execution_id.0.to_string()),
            attr("reflow.status", format!("{:?}", trace.status)),
        ],
    }));
    // One span per event.
    for e in &trace.events {
        spans.push(build_event_span(&trace_id_hex, &root_span_id, e));
    }

    json!({
        "resourceSpans": [{
            "resource": { "attributes": [ attr("service.name", service_name) ] },
            "scopeSpans": [{
                "scope": { "name": "reflow_tracing", "version": env!("CARGO_PKG_VERSION") },
                "spans": spans,
            }],
        }],
    })
}

/// Exports finalized traces to an OTLP/HTTP collector.
#[cfg(feature = "otlp")]
pub struct OtlpExporter {
    traces_url: String,
    service_name: String,
    client: reqwest::Client,
}

#[cfg(feature = "otlp")]
impl OtlpExporter {
    pub fn new(config: OtlpConfig) -> Result<Self> {
        let base = config.endpoint.trim_end_matches('/');
        Ok(Self {
            traces_url: format!("{base}/v1/traces"),
            service_name: config.service_name,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    /// Best-effort export of one finalized trace. Errors are returned for the
    /// caller to log; export should never block the data plane.
    pub async fn export_trace(&self, trace: &FlowTrace) -> Result<()> {
        let body = flow_trace_to_otlp_json(trace, &self.service_name);
        let resp = self.client.post(&self.traces_url).json(&body).send().await?;
        if !resp.status().is_success() {
            let code = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OTLP export rejected ({code}): {text}");
        }
        Ok(())
    }
}

// Stub when the feature is disabled, so the server can hold an
// `Option<OtlpExporter>` unconditionally.
#[cfg(not(feature = "otlp"))]
pub struct OtlpExporter;

#[cfg(not(feature = "otlp"))]
impl OtlpExporter {
    pub fn new(_config: OtlpConfig) -> Result<Self> {
        anyhow::bail!("OTLP export not compiled in — build reflow_tracing with --features otlp")
    }
    pub async fn export_trace(&self, _trace: &FlowTrace) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use reflow_tracing_protocol::{
        ExecutionId, ExecutionStatus, FlowId, FlowVersion, MessageSnapshot, PerformanceMetrics,
        TraceEvent, TraceId, TraceMetadata,
    };
    use std::collections::HashMap;

    fn trace() -> FlowTrace {
        let mut df = TraceEvent::data_flow(
            "reader".into(),
            "out".into(),
            "writer".into(),
            "in".into(),
            MessageSnapshot::capture("String", &serde_json::json!("hi"), true, false),
            PerformanceMetrics::cheap(Some(1_500_000), Some(2)),
        );
        df.causality.parent_event_id = None;
        FlowTrace {
            trace_id: TraceId::new(),
            flow_id: FlowId::new("flow_x"),
            execution_id: ExecutionId::new(),
            version: FlowVersion {
                major: 1,
                minor: 0,
                patch: 0,
                git_hash: None,
                timestamp: Utc::now(),
            },
            start_time: Utc::now(),
            end_time: Some(Utc::now()),
            status: ExecutionStatus::Completed,
            events: vec![TraceEvent::actor_created("reader".into()), df],
            metadata: TraceMetadata {
                user_id: None,
                session_id: None,
                environment: "test".into(),
                hostname: "h".into(),
                process_id: 1,
                thread_id: "t".into(),
                tags: HashMap::new(),
            },
        }
    }

    #[test]
    fn maps_flow_trace_to_valid_otlp_json() {
        let t = trace();
        let v = flow_trace_to_otlp_json(&t, "reflow-test");

        // trace id is the 128-bit reflow uuid as 32 lowercase hex chars.
        let spans = &v["resourceSpans"][0]["scopeSpans"][0]["spans"];
        let trace_id = spans[0]["traceId"].as_str().unwrap();
        assert_eq!(trace_id.len(), 32);
        assert_eq!(trace_id, &t.trace_id.0.simple().to_string());

        // root span + one span per event.
        assert_eq!(spans.as_array().unwrap().len(), t.events.len() + 1);
        // root has no parent; every span id is 16 hex chars.
        assert!(spans[0].get("parentSpanId").is_none());
        for s in spans.as_array().unwrap() {
            assert_eq!(s["spanId"].as_str().unwrap().len(), 16);
            assert_eq!(s["traceId"].as_str().unwrap(), trace_id);
        }

        // service.name resource attribute is set.
        let rattrs = &v["resourceSpans"][0]["resource"]["attributes"];
        assert_eq!(rattrs[0]["key"], "service.name");
        assert_eq!(rattrs[0]["value"]["stringValue"], "reflow-test");

        // the data-flow span carries the content checksum + destination.
        let df = spans
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"].as_str().unwrap_or("").starts_with("dataflow:"))
            .expect("data-flow span");
        let attrs = df["attributes"].as_array().unwrap();
        let has = |k: &str| attrs.iter().any(|a| a["key"] == k);
        assert!(has("reflow.to_actor"));
        assert!(has("reflow.message.checksum"));
        // execution_time_ns (1.5ms) makes end > start.
        assert!(
            df["endTimeUnixNano"].as_str().unwrap() > df["startTimeUnixNano"].as_str().unwrap()
        );
    }

    /// Live POST to a real OTLP/HTTP collector (Monoscope, an OTel collector on
    /// :4318, …). Runs only with `--features otlp` and `REFLOW_TEST_OTLP_ENDPOINT`.
    #[cfg(feature = "otlp")]
    #[tokio::test]
    async fn live_export_to_collector() {
        let endpoint = match std::env::var("REFLOW_TEST_OTLP_ENDPOINT") {
            Ok(e) if !e.is_empty() => e,
            _ => {
                eprintln!("skipping OTLP live test — set REFLOW_TEST_OTLP_ENDPOINT (e.g. http://localhost:4318)");
                return;
            }
        };
        let exporter = OtlpExporter::new(OtlpConfig {
            enabled: true,
            endpoint,
            service_name: "reflow-otlp-test".into(),
        })
        .unwrap();
        exporter.export_trace(&trace()).await.expect("export to collector");
    }
}
