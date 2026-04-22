# reflow_rt_capi

C ABI bindings for the Reflow runtime — the native surface consumed by non-Rust SDKs.

> **Rust users should depend on [`reflow_rt`](https://docs.rs/reflow_rt) directly.** This crate exists so that SDKs in other languages share a single canonical ABI: Go via `cgo`, other ecosystems either bind the `.h` directly or wrap it in an idiomatic layer.

## What it provides

- A stable, C-compatible ABI around `reflow_rt`: network lifecycle, graph construction / loading, event streaming.
- `cdylib` + `staticlib` build artifacts so consumers can dynamically link or bundle the runtime.
- Auto-generated `include/reflow_rt.h` (via `cbindgen`).

## Building

```sh
# Regular build — produces libreflow_rt_capi.{dylib,so,dll} + .a
cargo build -p reflow_rt_capi --release

# Regenerate the C header (requires the optional cbindgen build-dep)
cargo build -p reflow_rt_capi --features generate-header
```

The header lands at `include/reflow_rt.h`.

## Conventions

All details are in `src/lib.rs`; the short version:

- **Handles** are opaque pointers (`rfl_network*`, `rfl_graph*`, `rfl_events*`). Every `*_new` / `*_load` has a matching `*_free` that is NULL-safe.
- **Return codes** use the `rfl_status` enum; out-parameters are written on success.
- **Strings** owned by the library are freed with `rfl_string_free`. Never pass them to `free(3)`.
- **Error messages** are thread-local; retrieve with `rfl_last_error_message` after a failing call.
- **Threading**: all handles are `Send + Sync`; the crate lazily spins up a shared tokio multi-thread runtime on first use.

## Current surface (v0.2)

### Graph lifecycle + builder

```c
rfl_graph* rfl_graph_new(const char* name, int case_sensitive);
rfl_graph* rfl_graph_load_json(const char* json);
void       rfl_graph_free(rfl_graph*);
char*      rfl_graph_to_json(rfl_graph*);            /* reverse of load_json */

rfl_status rfl_graph_add_node     (rfl_graph*, const char* id, const char* component, const char* metadata_json);
rfl_status rfl_graph_remove_node  (rfl_graph*, const char* id);
rfl_status rfl_graph_set_node_metadata(rfl_graph*, const char* id, const char* metadata_json);

rfl_status rfl_graph_add_connection   (rfl_graph*, const char* out_node, const char* out_port,
                                                   const char* in_node,  const char* in_port,
                                                   const char* metadata_json);
rfl_status rfl_graph_remove_connection(rfl_graph*, const char* out_node, const char* out_port,
                                                   const char* in_node,  const char* in_port);

rfl_status rfl_graph_add_initial    (rfl_graph*, const char* node, const char* port,
                                                 const char* data_json, const char* metadata_json);
rfl_status rfl_graph_remove_initial (rfl_graph*, const char* node, const char* port);

rfl_status rfl_graph_add_inport    (rfl_graph*, const char* port_id, const char* node_id,
                                                const char* port_key, const char* port_type_json,
                                                const char* metadata_json);
rfl_status rfl_graph_add_outport   (rfl_graph*, const char* port_id, const char* node_id,
                                                const char* port_key, const char* port_type_json,
                                                const char* metadata_json);
rfl_status rfl_graph_remove_inport (rfl_graph*, const char* port_id);
rfl_status rfl_graph_remove_outport(rfl_graph*, const char* port_id);
```

### Network lifecycle + builder

```c
rfl_network* rfl_network_new(void);
rfl_network* rfl_network_from_graph(rfl_graph*);      /* consumes the graph */
rfl_status   rfl_network_start   (rfl_network*);
rfl_status   rfl_network_shutdown(rfl_network*);
void         rfl_network_free    (rfl_network*);

rfl_status rfl_network_add_node      (rfl_network*, const char* id, const char* template_id, const char* config_json);
rfl_status rfl_network_add_connection(rfl_network*, const char* from_actor, const char* from_port,
                                                    const char* to_actor,   const char* to_port);
rfl_status rfl_network_add_initial   (rfl_network*, const char* actor, const char* port, const char* message_json);
```

### Event stream

```c
rfl_events*  rfl_network_events(rfl_network*);
rfl_status   rfl_events_recv   (rfl_events*, uint32_t timeout_ms, char** out_json);
void         rfl_events_free   (rfl_events*);
```

### Misc

```c
char* rfl_version(void);
char* rfl_last_error_message(void);
void  rfl_string_free(char*);
void  rfl_runtime_shutdown(void);
```

### Callback-driven actors

Language SDKs can register native functions as actors. The runtime calls
them on every tick with a per-call `rfl_actor_ctx*` that exposes inputs,
config, state, and an emitter.

```c
typedef enum rfl_status (*rfl_actor_fn)(void* user_data, rfl_actor_ctx* ctx);
typedef void            (*rfl_actor_drop_fn)(void* user_data);

rfl_actor* rfl_actor_new(const char* component_name,
                         const char* const* inports,  size_t n_inports,
                         const char* const* outports, size_t n_outports,
                         int await_all_inports,
                         rfl_actor_fn callback,
                         void* user_data,
                         rfl_actor_drop_fn user_data_drop); /* may be NULL */
void       rfl_actor_free(rfl_actor*);

rfl_status rfl_network_register_actor(rfl_network*, const char* template_id, rfl_actor*);
```

Inside the callback (all returned strings are `rfl_string_free`-owned):

```c
int        rfl_ctx_has_input   (rfl_actor_ctx*, const char* port);
char*      rfl_ctx_input_json  (rfl_actor_ctx*, const char* port);
char*      rfl_ctx_config_json (rfl_actor_ctx*);
char*      rfl_ctx_state_get   (rfl_actor_ctx*, const char* key);
rfl_status rfl_ctx_state_set   (rfl_actor_ctx*, const char* key, const char* value_json);
rfl_status rfl_ctx_emit        (rfl_actor_ctx*, const char* port, const char* message_json);
```

Threading: callbacks run on a tokio worker. Language SDKs must arrange
their own synchronization (Node ThreadSafeFunction, Python GIL acquire,
JNI `AttachCurrentThread`, etc.).

`user_data_drop` fires exactly once, when the last reference to the actor
is released by the runtime — use it to release your GC root.

### Typed messages

Skip the JSON tax for hot paths. `rfl_message*` is an opaque handle; pass
it to `rfl_ctx_emit_message` or `rfl_ctx_take_input_message` on the way
in and out.

```c
rfl_message* rfl_message_flow(void);
rfl_message* rfl_message_boolean(int);
rfl_message* rfl_message_integer(int64_t);
rfl_message* rfl_message_float(double);
rfl_message* rfl_message_string(const char*);
rfl_message* rfl_message_bytes(const uint8_t*, size_t);
rfl_message* rfl_message_object_from_json(const char*);
rfl_message* rfl_message_array_from_json(const char*);
rfl_message* rfl_message_error(const char*);
rfl_message* rfl_message_from_json(const char*);    /* fallback */

rfl_message_kind rfl_message_get_kind(const rfl_message*);
int   rfl_message_as_boolean (const rfl_message*, int* out);
int   rfl_message_as_integer (const rfl_message*, int64_t* out);
int   rfl_message_as_float   (const rfl_message*, double* out);
char* rfl_message_as_string  (const rfl_message*);     /* rfl_string_free */
char* rfl_message_as_json    (const rfl_message*);     /* rfl_string_free */
int   rfl_message_bytes_borrow(const rfl_message*, const uint8_t** out, size_t* len);
void  rfl_message_free(rfl_message*);

rfl_status   rfl_ctx_emit_message     (rfl_actor_ctx*, const char* port, rfl_message*); /* transfers ownership */
rfl_message* rfl_ctx_take_input_message(rfl_actor_ctx*, const char* port);
```

### Streams

Producer + consumer sides for `Message::StreamHandle`. Frames travel
through bounded flume channels; the message carries only the handle.

```c
rfl_stream*  rfl_stream_new(size_t buffer_size, const char* origin_actor,
                            const char* origin_port, const char* content_type);
rfl_status   rfl_stream_send_begin(rfl_stream*, const char* content_type,
                                   uint64_t size_hint, int has_size_hint,
                                   const char* metadata_json);
rfl_status   rfl_stream_send_bytes(rfl_stream*, const uint8_t*, size_t);
rfl_status   rfl_stream_end       (rfl_stream*);
rfl_status   rfl_stream_error     (rfl_stream*, const char* message);
rfl_message* rfl_stream_into_message(rfl_stream*);   /* consumes, emit via rfl_ctx_emit_message */
void         rfl_stream_free     (rfl_stream*);

rfl_stream_recv* rfl_message_stream_take(rfl_message*);    /* one-shot */
rfl_status       rfl_stream_recv_next   (rfl_stream_recv*, uint32_t timeout_ms,
                                         rfl_stream_frame_kind* out_kind,
                                         const uint8_t** out_data, size_t* out_len,
                                         char** out_err);
void             rfl_stream_recv_free   (rfl_stream_recv*);
```

### Subgraph builder

For subgraphs whose referenced components aren't all in the bundled
catalog (e.g. callback actors registered by the SDK):

```c
rfl_subgraph_builder* rfl_subgraph_builder_new(const char* graph_export_json);
rfl_status            rfl_subgraph_builder_register_actor(rfl_subgraph_builder*,
                                                          const char* component_name,
                                                          rfl_actor*);     /* transfers ownership */
rfl_status            rfl_subgraph_builder_fill_from_catalog(rfl_subgraph_builder*);
rfl_actor*            rfl_subgraph_builder_build(rfl_subgraph_builder*);   /* consumes builder */
void                  rfl_subgraph_builder_free(rfl_subgraph_builder*);
```

### NetworkConfig

`rfl_network_new_with_config(const char* json)` — parse a serialized
`NetworkConfig`. Default is still available via `rfl_network_new()`.

### Template catalog + subgraph (gated on the `components` feature)

Enabled by default; compile with `--no-default-features` to drop
`reflow_components` for a thinner shared library.

```c
/* Bundled component instantiation */
rfl_actor* rfl_template_actor_new(const char* template_id);
char*      rfl_template_list_json(void);         /* JSON array of ids */

/* Subgraph embedding — resolves referenced components from the catalog */
rfl_actor* rfl_subgraph_actor_new_from_json(const char* graph_export_json);
```

All three return an `rfl_actor*` you can hand to
`rfl_network_register_actor`. This is how a non-Rust SDK registers the
bundled standard library (~300 templates) and composes graph exports as
first-class subgraph nodes.

### Not yet in the ABI

- **Structured event types** — events currently arrive as JSON via
  `rfl_events_recv`. Could be promoted to an `rfl_event*` handle with
  typed accessors; deferred until an SDK asks.
- **Graph introspection without JSON round-trip** — today SDKs call
  `rfl_graph_to_json` and parse client-side. Fine for cold paths; not
  fine for frequent reads. Could add `rfl_graph_node_ids`,
  `rfl_graph_node_component`, `rfl_graph_connections` when that's a
  real hotspot.
- **Template metadata introspection** — port shapes, description,
  config schema. Currently accessible only by instantiating the actor
  or consulting the published catalog docs.

## License

MIT OR Apache-2.0.
