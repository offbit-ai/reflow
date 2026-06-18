# SQLite Integration Guide

SQLite is the **default durable backend** — a single embedded file, no server to
run. Ideal for development, single-node deployments, and anywhere you want
persistence without operating a database.

## Build

SQLite ships in the default build (the `storage` feature is on by default):

```bash
cargo build -p reflow_tracing            # SQLite included
```

## Configure

Set `storage.backend = "sqlite"` and a `[storage.sqlite]` block. All fields:

```toml
[storage]
backend = "sqlite"

[storage.sqlite]
database_path = "traces.db"   # file path, or ":memory:" for an ephemeral DB
wal_mode      = true          # Write-Ahead Logging (recommended)
journal_mode  = "WAL"
synchronous   = "NORMAL"      # NORMAL is a good durability/speed balance
cache_size    = -2000         # KB when negative (here ~2 MB), pages when positive
```

The database file is **created automatically** if it doesn't exist (the server
opens it with `mode=rwc`); the parent directory must already exist.

## Schema

Created on startup — you never run DDL by hand. Each `FlowTrace` is stored as one
row in `traces`: the full trace is a (zstd-compressed when large) JSON blob in
`data`, alongside denormalized columns for querying:

| column | purpose |
|---|---|
| `trace_id` (PK) | trace identity |
| `flow_id`, `execution_id`, `status` | query filters |
| `start_time`, `end_time` | time-range queries |
| `data` (BLOB), `compressed`, `size_bytes` | the trace payload |

Indexes cover `flow_id`, `execution_id`, `status`, `start_time`. Writes are
**synchronous (write-through)**, so a trace is queryable the instant it's stored.

## Verify

```bash
# point the server at a sqlite config and start it
reflow_tracing            # logs: "Initialized storage backend: sqlite"

# after some traces have flowed, inspect the file directly
sqlite3 traces.db '.tables'                       # traces, trace_events
sqlite3 traces.db 'SELECT count(*) FROM traces;'
```

Or query through the protocol (`get_trace` / `query_traces`) from any SDK / the
monitoring client.

## Operations

- **Backups**: it's a single file — copy it (use the SQLite backup API or copy
  while WAL-checkpointed). The WAL (`traces.db-wal`) must travel with the file.
- **Concurrency**: SQLite is single-writer. Fine for one collector; for high
  concurrent write volume move to [PostgreSQL](postgres.md) /
  [TimescaleDB](timescaledb.md).
- **Retention**: SQLite has no automatic TTL. Prune with
  `DELETE FROM traces WHERE start_time < strftime('%s','now','-30 days')` on a
  schedule, or use the in-process delete API.
- **Cache**: bump `cache_size` (negative = KB) for read-heavy workloads.

## Troubleshooting

- **`unable to open database file`** — the parent directory doesn't exist, or the
  process can't write there. Create the directory / fix permissions.
- **No traces appear** — confirm `backend = "sqlite"` (not `memory`) and that the
  network's `TracingConfig.enabled = true` points at this server.
- **`Storage feature is not enabled`** — you built with `--no-default-features`;
  re-enable the `storage` feature.
