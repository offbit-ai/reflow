# TimescaleDB Integration Guide

TimescaleDB is **PostgreSQL plus a time-series extension** — same protocol, same
driver. Traces are inherently time-series (every trace has a `start_time`), so
the `timescale` backend converts the `traces` table into a **hypertable**
partitioned on `start_time`: time-chunked storage, fast time-range scans, and
native compression/retention policies.

Because it speaks the Postgres protocol, it **reuses the
[`[storage.postgres]`](postgres.md) connection config** — only the backend name
differs.

## Build

Uses the same feature as PostgreSQL:

```bash
cargo build -p reflow_tracing --features postgres
```

## Run a database

```bash
docker run -d --name reflow-ts \
  -e POSTGRES_DB=traces -e POSTGRES_USER=reflow -e POSTGRES_PASSWORD=secret \
  -p 5432:5432 timescale/timescaledb:latest-pg16
```

## Configure

Set the backend to `timescale` (or `timescaledb`); the connection config lives
under `[storage.postgres]`:

```toml
[storage]
backend = "timescale"

[storage.postgres]
connection_url       = "postgresql://reflow:secret@localhost:5432/traces"
max_connections      = 20
min_connections      = 5
acquire_timeout_secs = 5
```

## What the server sets up

On startup it creates the `traces` table, then best-effort runs:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;
SELECT create_hypertable('traces', 'start_time',
       chunk_time_interval => 604800,        -- 7-day chunks (start_time is unix seconds)
       if_not_exists => TRUE, migrate_data => TRUE);
```

Key differences from plain Postgres:

- the schema keys on **`(trace_id, start_time)`** — a hypertable's unique key must
  include the partition column. `start_time` is set once per trace, so the key is
  stable and the `ON CONFLICT (trace_id, start_time)` upsert is idempotent.
- a non-unique index on `trace_id` keeps point lookups (`get`/`delete`) fast.
- **graceful degradation**: if the `timescaledb` extension isn't installed, the
  server logs a warning and continues with a plain (composite-key) table — still
  correct, just not time-partitioned.

## Verify

```bash
reflow_tracing            # logs: "Initialized storage backend: timescale"

# confirm the hypertable exists
psql "$CONN" -c "SELECT hypertable_name FROM timescaledb_information.hypertables;"
```

Live integration test:

```bash
REFLOW_TEST_TIMESCALE_URL="postgresql://reflow:secret@localhost:5432/traces" \
  cargo test -p reflow_tracing --features postgres --test storage_backends \
  timescaledb_store_query_delete_roundtrip
```

## Operations — the payoff

These are the reasons to pick TimescaleDB; configure them as policies on the
hypertable:

```sql
-- Native retention: drop chunks older than 30 days, automatically.
SELECT add_retention_policy('traces', INTERVAL '30 days');

-- Native compression of older chunks.
ALTER TABLE traces SET (timescaledb.compress);
SELECT add_compression_policy('traces', INTERVAL '7 days');
```

Time-range queries (`query_traces` with a `time_range`) hit only the relevant
chunks. Tune `chunk_time_interval` for your trace volume if 7 days isn't ideal.

## Troubleshooting

- **Hypertable warning in logs** — the extension isn't installed; either install
  it (`timescale/timescaledb` image) or accept the plain-table fallback.
- **`TimescaleDB uses the [storage.postgres] config; it is missing`** — add the
  `[storage.postgres]` block.
- Everything else mirrors the [PostgreSQL guide](postgres.md).
