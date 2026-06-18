# MongoDB Integration Guide

MongoDB is a **document-store** backend — the JSON-shaped `FlowTrace` maps
naturally onto a BSON document. A good fit if you already operate MongoDB, want
schema-less retention of the evolving trace shape, or scale horizontally via
sharding.

## Build

Behind a cargo feature (adds the `mongodb` + `bson` drivers):

```bash
cargo build -p reflow_tracing --features mongodb
# or every durable backend at once:
cargo build -p reflow_tracing --features all-backends
```

## Run a database

```bash
docker run -d --name reflow-mongo -p 27017:27017 mongo:7
```

## Configure

Set `storage.backend` to `mongodb` (or `mongo`) and a `[storage.mongodb]` block.
All fields:

```toml
[storage]
backend = "mongodb"

[storage.mongodb]
connection_url  = "mongodb://localhost:27017"
database_name   = "reflow_tracing"
collection_name = "traces"
```

## Document model

Each `FlowTrace` is one document keyed by its `trace_id` (`_id`), with
denormalized top-level fields for querying and the full trace nested under
`trace`:

```json
{
  "_id": "<trace_id>",
  "flow_id": "...", "execution_id": "...", "status": "...",
  "start_time": 1718000000, "end_time": 1718000005, "event_count": 12,
  "trace": { /* the full FlowTrace */ }
}
```

Stores use an upsert (`replace_one … upsert`), and the server creates indexes on
`flow_id`, `execution_id`, `status`, and `start_time` on startup.

## Verify

```bash
reflow_tracing            # logs: "Initialized storage backend: mongodb"

mongosh "mongodb://localhost:27017/reflow_tracing" \
  --eval 'db.traces.countDocuments()'
mongosh "mongodb://localhost:27017/reflow_tracing" \
  --eval 'db.traces.findOne({}, {flow_id:1, status:1, event_count:1})'
```

Live integration test:

```bash
REFLOW_TEST_MONGODB_URL="mongodb://localhost:27017" \
  cargo test -p reflow_tracing --features mongodb --test storage_backends \
  mongodb_store_query_delete_roundtrip
```

## Operations

- **Retention (TTL)**: a TTL index expires old traces automatically — e.g. add a
  BSON `Date` field and
  `db.traces.createIndex({ expires_at: 1 }, { expireAfterSeconds: 0 })`, or run a
  periodic `deleteMany({ start_time: { $lt: cutoff } })`.
- **Sharding**: shard on `_id` (trace_id) or `flow_id` for horizontal scale.
- **Backups**: `mongodump` / replica sets.

## Troubleshooting

- **`MongoDB backend not compiled in`** — rebuild with `--features mongodb`.
- **`MongoDB storage config missing`** — `backend = "mongodb"` but no
  `[storage.mongodb]` block.
- **Connection errors** — check `connection_url`, that the server is up, and auth
  (`mongodb://user:pass@host:27017`).
