# PostgreSQL Integration Guide

PostgreSQL is the recommended backend for **production, multi-instance, and
high-concurrency** deployments — a real server with connection pooling and strong
durability. For time-series-heavy workloads, see the
[TimescaleDB guide](timescaledb.md), which reuses this same config.

## Build

PostgreSQL is behind a cargo feature (adds the sqlx Postgres driver):

```bash
cargo build -p reflow_tracing --features postgres
# or every durable backend at once:
cargo build -p reflow_tracing --features all-backends
```

## Run a database

```bash
docker run -d --name reflow-pg \
  -e POSTGRES_DB=traces -e POSTGRES_USER=reflow -e POSTGRES_PASSWORD=secret \
  -p 5432:5432 postgres:16
```

## Configure

Set `storage.backend` to `postgres` (or `postgresql`) and a `[storage.postgres]`
block. All fields:

```toml
[storage]
backend = "postgres"

[storage.postgres]
connection_url       = "postgresql://reflow:secret@localhost:5432/traces"
max_connections      = 20
min_connections      = 5
acquire_timeout_secs = 5
```

## Schema

Created automatically on startup (the `traces` table + indexes) — no manual DDL.
Each `FlowTrace` is one row: the full trace as a (zstd-compressed when large)
JSON `BYTEA` blob, plus denormalized columns:

| column | type | purpose |
|---|---|---|
| `trace_id` (PK) | `TEXT` | identity |
| `flow_id`, `execution_id`, `status` | `TEXT` | query filters |
| `start_time`, `end_time` | `BIGINT` | time-range queries |
| `event_count` | `BIGINT` | stats without loading the blob |
| `data`, `compressed`, `size_bytes` | `BYTEA`/`BOOL`/`BIGINT` | payload |

Stores are **synchronous** with an `INSERT … ON CONFLICT (trace_id) DO UPDATE`
upsert (idempotent re-finalize), and indexes cover `flow_id`, `execution_id`,
`status`, `start_time`.

## Verify

```bash
reflow_tracing            # logs: "Initialized storage backend: postgres"

psql "$CONN" -c '\dt'                          # traces
psql "$CONN" -c 'SELECT count(*) FROM traces;'
```

Run the bundled integration test against a live instance:

```bash
REFLOW_TEST_POSTGRES_URL="postgresql://reflow:secret@localhost:5432/traces" \
  cargo test -p reflow_tracing --features postgres --test storage_backends \
  postgres_store_query_delete_roundtrip
```

## Operations

- **Pooling**: size `max_connections` to your collector concurrency; `sqlx`
  manages the pool with `acquire_timeout_secs`.
- **Retention**: schedule
  `DELETE FROM traces WHERE start_time < EXTRACT(EPOCH FROM now() - INTERVAL '30 days')`,
  or use [TimescaleDB](timescaledb.md) for native retention.
- **Backups**: standard `pg_dump` / streaming replication / PITR.
- **Scale**: read replicas, partitioning. If your queries are dominated by
  time ranges, TimescaleDB's hypertable is a better fit.

## Troubleshooting

- **`PostgreSQL backend not compiled in`** — rebuild with `--features postgres`.
- **`PostgreSQL storage config missing`** — `backend = "postgres"` but no
  `[storage.postgres]` block.
- **Connection refused / auth failed** — check `connection_url`, that the server
  is reachable, and credentials.
