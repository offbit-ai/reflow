//! `reflow trace …` — run the collector or consume traces.

use anyhow::{Context, Result};
use clap::Subcommand;
use reflow_tracing_protocol::client::{TracingClient, TracingConfig};
use reflow_tracing_protocol::{FlowId, SubscriptionFilters, TraceId, TraceQuery};
use std::path::PathBuf;

const DEFAULT_SERVER: &str = "ws://localhost:8080";

#[derive(Subcommand)]
pub enum TraceCmd {
    /// Run the tracing collector server.
    Serve {
        /// Optional config file (TOML); otherwise uses the standard search path.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Stream live trace events from a collector.
    Tail {
        #[arg(long, default_value = DEFAULT_SERVER)]
        server: String,
        /// Filter by flow id (repeatable).
        #[arg(long = "flow-id")]
        flow_ids: Vec<String>,
        /// Filter by actor id (repeatable).
        #[arg(long = "actor-id")]
        actor_ids: Vec<String>,
    },
    /// Query historical traces.
    Query {
        #[arg(long, default_value = DEFAULT_SERVER)]
        server: String,
        #[arg(long = "flow-id")]
        flow_id: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Fetch a single trace by id.
    Get {
        trace_id: String,
        #[arg(long, default_value = DEFAULT_SERVER)]
        server: String,
    },
}

pub async fn run(cmd: TraceCmd) -> Result<()> {
    match cmd {
        TraceCmd::Serve { config } => serve(config).await,
        TraceCmd::Tail {
            server,
            flow_ids,
            actor_ids,
        } => tail(server, flow_ids, actor_ids).await,
        TraceCmd::Query {
            server,
            flow_id,
            limit,
        } => query(server, flow_id, limit).await,
        TraceCmd::Get { trace_id, server } => get(server, trace_id).await,
    }
}

async fn serve(config: Option<PathBuf>) -> Result<()> {
    use reflow_tracing::config::Config;
    use reflow_tracing::server::TraceServer;

    let cfg = match config {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            toml::from_str::<Config>(&text)
                .with_context(|| format!("parsing {}", path.display()))?
        }
        None => Config::load().unwrap_or_else(|_| {
            tracing::warn!("no tracing config found; using defaults");
            Config::default()
        }),
    };

    let addr: std::net::SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .context("invalid server host/port")?;
    let server = TraceServer::new(cfg).await?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("tracing collector listening on {}", addr);
    server.run(listener).await
}

fn client(server: &str) -> TracingClient {
    TracingClient::new(TracingConfig {
        server_url: server.to_string(),
        ..TracingConfig::default()
    })
}

async fn tail(server: String, flow_ids: Vec<String>, actor_ids: Vec<String>) -> Result<()> {
    let c = client(&server);
    c.connect().await.context("connecting to collector")?;

    let (tx, rx) = flume::unbounded();
    c.set_notification_tap(tx);
    c.subscribe(SubscriptionFilters {
        flow_ids: (!flow_ids.is_empty()).then(|| flow_ids.into_iter().map(FlowId::new).collect()),
        actor_ids: (!actor_ids.is_empty()).then_some(actor_ids),
        event_types: None,
        status_filter: None,
    })
    .await
    .context("subscribing")?;

    eprintln!("reflow: tailing {server} (Ctrl-C to stop)");
    loop {
        tokio::select! {
            evt = rx.recv_async() => match evt {
                Ok(evt) => {
                    if let Ok(json) = serde_json::to_string(&evt) { println!("{json}"); }
                }
                Err(_) => break,
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

async fn query(server: String, flow_id: Option<String>, limit: usize) -> Result<()> {
    let c = client(&server);
    c.connect().await.context("connecting to collector")?;
    let traces = c
        .query_traces(TraceQuery {
            flow_id: flow_id.map(FlowId::new),
            execution_id: None,
            time_range: None,
            status: None,
            actor_filter: None,
            limit: Some(limit),
            offset: None,
        })
        .await
        .context("querying traces")?;
    for t in &traces {
        println!("{}", serde_json::to_string(t)?);
    }
    eprintln!("reflow: {} trace(s)", traces.len());
    Ok(())
}

async fn get(server: String, trace_id: String) -> Result<()> {
    let id = TraceId(
        uuid::Uuid::parse_str(&trace_id).with_context(|| format!("invalid trace id {trace_id:?}"))?,
    );
    let c = client(&server);
    c.connect().await.context("connecting to collector")?;
    match c.get_trace(id).await.context("get_trace")? {
        Some(t) => println!("{}", serde_json::to_string_pretty(&t)?),
        None => eprintln!("reflow: no trace {trace_id}"),
    }
    Ok(())
}
