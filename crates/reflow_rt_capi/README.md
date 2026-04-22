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

### Not yet in the ABI (next passes)

- **Callback actors**: `rfl_actor_register`, `rfl_network_register_actor`, `rfl_actor_ctx`, and the message/state handle types language SDKs invoke from inside a callback.
- **Message / state / stream-handle accessors**: `rfl_message_*`, `rfl_state_*`, `rfl_stream_*`.
- **Template catalog**: `rfl_template_actor_new`, `rfl_template_mapping` — needed for SDKs that want to register the bundled `reflow_components` actors by id.
- **Subgraph embedding**: `rfl_subgraph_new` over a `GraphExport`.

## License

MIT OR Apache-2.0.
