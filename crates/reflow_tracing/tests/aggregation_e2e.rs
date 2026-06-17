//! Phase 1 end-to-end: a stream of `RecordEvent`s sent under one client-owned
//! `trace_id` must be aggregated by the server into a single `FlowTrace` that
//! `get_trace` and `query_traces` return — i.e. correlation + persistence +
//! request/response correlation all work over the real WebSocket client.

use std::time::Duration;

use reflow_tracing::config::Config;
use reflow_tracing::server::TraceServer;
use reflow_tracing_protocol::client::{TracingClient, TracingConfig};
use reflow_tracing_protocol::{
    ExecutionStatus, FlowId, FlowTrace, FlowVersion, TraceEvent, TraceId, TraceQuery,
};
use tokio::net::TcpListener;

/// Bind an in-process tracing server on an ephemeral port and return its ws URL.
async fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = TraceServer::new(Config::default()).await.unwrap();
    tokio::spawn(async move {
        let _ = server.run(listener).await;
    });
    format!("ws://{}", addr)
}

fn version() -> FlowVersion {
    FlowVersion {
        major: 1,
        minor: 0,
        patch: 0,
        git_hash: None,
        timestamp: chrono::Utc::now(),
    }
}

async fn poll_get(client: &TracingClient, trace_id: &TraceId) -> Option<FlowTrace> {
    for _ in 0..30 {
        if let Ok(Some(t)) = client.get_trace(trace_id.clone()).await {
            return Some(t);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[tokio::test]
async fn records_and_queries_a_correlated_trace() {
    let url = start_server().await;

    let client = TracingClient::new(TracingConfig {
        server_url: url,
        ..TracingConfig::default()
    });
    client.connect().await.expect("connect to tracing server");
    // Let the connection's writer task install its message sender.
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Start a flow trace — the client owns the id.
    let flow_id = FlowId::new("phase1_test_flow");
    let trace_id = client
        .start_trace(flow_id.clone(), version())
        .await
        .expect("start_trace");

    // Three correlated events under the same trace id.
    client
        .record_event(trace_id.clone(), TraceEvent::actor_created("reader".into()))
        .await
        .unwrap();
    client
        .record_event(
            trace_id.clone(),
            TraceEvent::data_flow(
                "reader".into(),
                "out".into(),
                "writer".into(),
                "in".into(),
                "String".into(),
                12,
            ),
        )
        .await
        .unwrap();
    client
        .record_event(
            trace_id.clone(),
            TraceEvent::actor_completed("writer".into()),
        )
        .await
        .unwrap();

    // Let the events flush, then finalize the trace.
    tokio::time::sleep(Duration::from_millis(200)).await;
    client
        .end_trace(trace_id.clone(), ExecutionStatus::Completed)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // get_trace returns the finalized trace with all three events correlated.
    let trace = poll_get(&client, &trace_id).await.expect("trace persisted");
    assert_eq!(trace.trace_id, trace_id);
    assert_eq!(trace.flow_id, flow_id);
    assert_eq!(
        trace.events.len(),
        3,
        "all events correlated into one trace, got {:?}",
        trace.events.iter().map(|e| &e.event_type).collect::<Vec<_>>()
    );
    assert!(matches!(trace.status, ExecutionStatus::Completed));

    // query_traces by flow_id also returns it.
    let results = client
        .query_traces(TraceQuery {
            flow_id: Some(flow_id.clone()),
            execution_id: None,
            time_range: None,
            status: None,
            actor_filter: None,
            limit: Some(10),
            offset: None,
        })
        .await
        .expect("query_traces");
    assert!(
        results.iter().any(|t| t.trace_id == trace_id),
        "trace must be findable by flow_id query"
    );
}

#[tokio::test]
async fn in_flight_trace_is_visible_before_finalize() {
    let url = start_server().await;

    let client = TracingClient::new(TracingConfig {
        server_url: url,
        ..TracingConfig::default()
    });
    client.connect().await.expect("connect");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let flow_id = FlowId::new("phase1_inflight_flow");
    let trace_id = client
        .start_trace(flow_id, version())
        .await
        .expect("start_trace");
    client
        .record_event(trace_id.clone(), TraceEvent::actor_created("a".into()))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // No EndTrace yet — the active (Running) trace must still be queryable.
    let trace = poll_get(&client, &trace_id).await.expect("in-flight trace");
    assert!(matches!(trace.status, ExecutionStatus::Running));
    assert_eq!(trace.events.len(), 1);
}
