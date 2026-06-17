//! Storage-backend integration tests.
//!
//! SQLite runs against a temp file (no external server). PostgreSQL runs only
//! when `REFLOW_TEST_POSTGRES_URL` is set to a reachable instance, and is
//! skipped otherwise.

use chrono::Utc;
use reflow_tracing::storage::TraceStorage;
use reflow_tracing_protocol::{
    ExecutionStatus, ExecutionId, FlowId, FlowTrace, FlowVersion, TraceEvent, TraceId,
    TraceMetadata, TraceQuery,
};
use std::collections::HashMap;

fn sample_trace(flow_id: &str) -> FlowTrace {
    FlowTrace {
        trace_id: TraceId::new(),
        flow_id: FlowId::new(flow_id),
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
        events: vec![
            TraceEvent::actor_created("reader".into()),
            TraceEvent::actor_completed("reader".into()),
        ],
        metadata: TraceMetadata {
            user_id: None,
            session_id: None,
            environment: "test".into(),
            hostname: "localhost".into(),
            process_id: std::process::id(),
            thread_id: "t".into(),
            tags: HashMap::new(),
        },
    }
}

fn empty_query(flow_id: Option<&str>) -> TraceQuery {
    TraceQuery {
        flow_id: flow_id.map(FlowId::new),
        execution_id: None,
        time_range: None,
        status: None,
        actor_filter: None,
        limit: Some(50),
        offset: None,
    }
}

#[tokio::test]
async fn sqlite_store_query_delete_roundtrip() {
    use reflow_tracing::config::SqliteConfig;
    use reflow_tracing::storage::sqlite::SqliteStorage;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("traces.db");
    let storage = SqliteStorage::new(SqliteConfig {
        database_path: path.to_string_lossy().into_owned(),
        wal_mode: true,
        journal_mode: "WAL".into(),
        synchronous: "NORMAL".into(),
        cache_size: -2000,
    })
    .await
    .expect("open sqlite storage");

    let trace = sample_trace("sqlite_flow");
    let id = storage.store_trace(trace.clone()).await.expect("store");

    // store_trace buffers; the background flush runs ~every second.
    let mut got = None;
    for _ in 0..40 {
        if let Some(t) = storage.get_trace(id.clone()).await.expect("get") {
            got = Some(t);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let got = got.expect("trace persisted to sqlite");
    assert_eq!(got.trace_id, id);
    assert_eq!(got.flow_id, trace.flow_id);
    assert_eq!(got.events.len(), 2);

    let results = storage
        .query_traces(empty_query(Some("sqlite_flow")))
        .await
        .expect("query");
    assert!(results.iter().any(|t| t.trace_id == id));

    assert!(storage.delete_trace(id.clone()).await.expect("delete"));
    assert!(storage.get_trace(id).await.expect("get after delete").is_none());
}

#[tokio::test]
async fn mongodb_store_query_delete_roundtrip() {
    let url = match std::env::var("REFLOW_TEST_MONGODB_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("skipping mongodb test — set REFLOW_TEST_MONGODB_URL to run");
            return;
        }
    };

    use reflow_tracing::config::MongoDbConfig;
    use reflow_tracing::storage::mongo::MongoStorage;

    let storage = MongoStorage::new(MongoDbConfig {
        connection_url: url,
        database_name: "reflow_tracing_test".into(),
        collection_name: "traces".into(),
    })
    .await
    .expect("connect mongodb");

    let trace = sample_trace("mongo_flow");
    let id = storage.store_trace(trace.clone()).await.expect("store");

    let got = storage
        .get_trace(id.clone())
        .await
        .expect("get")
        .expect("trace present");
    assert_eq!(got.trace_id, id);
    assert_eq!(got.events.len(), 2);

    let results = storage
        .query_traces(empty_query(Some("mongo_flow")))
        .await
        .expect("query");
    assert!(results.iter().any(|t| t.trace_id == id));

    assert!(storage.delete_trace(id.clone()).await.expect("delete"));
    assert!(storage.get_trace(id).await.expect("get after delete").is_none());
}

#[tokio::test]
async fn postgres_store_query_delete_roundtrip() {
    let url = match std::env::var("REFLOW_TEST_POSTGRES_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            eprintln!("skipping postgres test — set REFLOW_TEST_POSTGRES_URL to run");
            return;
        }
    };

    use reflow_tracing::config::PostgresConfig;
    use reflow_tracing::storage::postgres::PostgresStorage;

    let storage = PostgresStorage::new(PostgresConfig {
        connection_url: url,
        max_connections: 4,
        min_connections: 1,
        acquire_timeout_secs: 5,
    })
    .await
    .expect("connect postgres");

    let trace = sample_trace("pg_flow");
    let id = storage.store_trace(trace.clone()).await.expect("store");

    // Postgres writes are synchronous — no flush wait needed.
    let got = storage
        .get_trace(id.clone())
        .await
        .expect("get")
        .expect("trace present");
    assert_eq!(got.trace_id, id);
    assert_eq!(got.events.len(), 2);

    let results = storage
        .query_traces(empty_query(Some("pg_flow")))
        .await
        .expect("query");
    assert!(results.iter().any(|t| t.trace_id == id));

    let stats = storage.get_stats().await.expect("stats");
    assert!(stats.total_traces >= 1);

    assert!(storage.delete_trace(id.clone()).await.expect("delete"));
    assert!(storage.get_trace(id).await.expect("get after delete").is_none());
}
