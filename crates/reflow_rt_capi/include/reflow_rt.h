#ifndef REFLOW_RT_H
#define REFLOW_RT_H

/* GENERATED FILE — edit src/lib.rs and rerun `cargo build --features generate-header`. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Variant tag — matches the `Message` enum in `reflow_actor`.
 */
typedef enum rfl_message_kind {
  rfl_message_kind_Flow = 0,
  rfl_message_kind_Boolean = 1,
  rfl_message_kind_Integer = 2,
  rfl_message_kind_Float = 3,
  rfl_message_kind_String = 4,
  rfl_message_kind_Object = 5,
  rfl_message_kind_Array = 6,
  rfl_message_kind_Bytes = 7,
  rfl_message_kind_Error = 8,
  rfl_message_kind_StreamHandle = 9,
  rfl_message_kind_Optional = 10,
  /**
   * Anything else — use `rfl_message_as_json` to inspect.
   */
  rfl_message_kind_Other = 99,
} rfl_message_kind;

typedef enum rfl_status {
  /**
   * Success.
   */
  rfl_status_Ok = 0,
  /**
   * A required argument was NULL.
   */
  rfl_status_NullArg = -1,
  /**
   * A C string was not valid UTF-8.
   */
  rfl_status_InvalidUtf8 = -2,
  /**
   * A JSON payload failed to parse or deserialize.
   */
  rfl_status_InvalidJson = -3,
  /**
   * The runtime refused the operation (see `rfl_last_error_message`).
   */
  rfl_status_Runtime = -4,
  /**
   * The network has already been started or is in a state that forbids
   * the operation.
   */
  rfl_status_InvalidState = -5,
} rfl_status;

/**
 * Variant tag returned by `rfl_stream_recv_next`.
 */
typedef enum rfl_stream_frame_kind {
  rfl_stream_frame_kind_Begin = 0,
  rfl_stream_frame_kind_Data = 1,
  rfl_stream_frame_kind_End = 2,
  rfl_stream_frame_kind_Error = 3,
  rfl_stream_frame_kind_Timeout = 4,
  rfl_stream_frame_kind_Closed = 5,
} rfl_stream_frame_kind;

/**
 * Opaque handle to an actor template. Produced by `rfl_actor_new`
 * (callback-driven actor), `rfl_template_actor_new` (bundled component),
 * or `rfl_subgraph_actor_new_from_json` (embedded subgraph). Hand to
 * `rfl_network_register_actor` to publish under a template id, or to
 * `rfl_actor_free` to release without registering.
 */
typedef struct rfl_actor rfl_actor;

/**
 * Opaque per-call context handed to a callback actor.
 */
typedef struct rfl_actor_ctx rfl_actor_ctx;

/**
 * Opaque handle to a subscriber on the network's event stream. One
 * subscriber per handle — create as many as you need.
 */
typedef struct rfl_events rfl_events;

/**
 * Opaque handle to a `reflow_graph::Graph`.
 */
typedef struct rfl_graph rfl_graph;

/**
 * Opaque Reflow message handle.
 */
typedef struct rfl_message rfl_message;

/**
 * Opaque handle to a `reflow_network::Network`, wrapped so the C side can
 * share it across threads without touching its internal `Arc<Mutex<_>>`.
 */
typedef struct rfl_network rfl_network;

/**
 * Opaque producer handle to a stream. Create with `rfl_stream_new`, hand
 * chunks in via `rfl_stream_send_bytes`, terminate with
 * `rfl_stream_end` / `rfl_stream_error`. Free with `rfl_stream_free`.
 */
typedef struct rfl_stream rfl_stream;

/**
 * Opaque receiver for a stream's data channel.
 */
typedef struct rfl_stream_recv rfl_stream_recv;

/**
 * Builder for a `SubgraphActor` with an explicit actor map.
 */
typedef struct rfl_subgraph_builder rfl_subgraph_builder;

/**
 * Function pointer: the body of a callback actor.
 *
 * The callback is invoked every time the runtime has inputs for the actor.
 * Return `rfl_status::Ok` to commit whatever was emitted via
 * `rfl_ctx_emit`; any other status aborts the tick with an error in the
 * network event stream.
 */
typedef enum rfl_status (*rfl_actor_fn)(void *user_data, struct rfl_actor_ctx *ctx);

/**
 * Function pointer: released when the runtime drops the last reference to
 * the actor. Use it to decrement a Node/Python/JVM GC root.
 */
typedef void (*rfl_actor_drop_fn)(void *user_data);

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Returns the last thread-local error message as a newly allocated C string.
 * Caller frees with `rfl_string_free`. Returns NULL if there is no error.
 */
char *rfl_last_error_message(void);

/**
 * Free a string returned by this library. Passing NULL is a no-op.
 */
void rfl_string_free(char *s);

/**
 * Explicitly tear down the shared tokio runtime.
 *
 * Safe to call more than once. Normally this happens automatically on
 * library unload; call it from long-lived embedders that want to release
 * threads early.
 */
void rfl_runtime_shutdown(void);

/**
 * Create a new empty graph.
 *
 * `name` and `case_sensitive` follow `Graph::new`. Pass NULL for `name`
 * to use an empty name.
 */
struct rfl_graph *rfl_graph_new(const char *name, int case_sensitive);

/**
 * Load a graph from a `GraphExport` JSON document.
 *
 * Returns NULL on failure (check `rfl_last_error_message`).
 */
struct rfl_graph *rfl_graph_load_json(const char *json);

/**
 * Free a graph handle. Safe on NULL.
 */
void rfl_graph_free(struct rfl_graph *g);

/**
 * Create a new network with `NetworkConfig::default()`.
 */
struct rfl_network *rfl_network_new(void);

/**
 * Create a new network from a serialized `NetworkConfig` JSON. Returns
 * NULL on parse error. Unknown fields are rejected.
 */
struct rfl_network *rfl_network_new_with_config(const char *config_json);

/**
 * Create a network from an already-loaded graph. The graph is consumed.
 */
struct rfl_network *rfl_network_from_graph(struct rfl_graph *g);

/**
 * Start the network. Non-blocking; returns immediately after scheduling
 * the actors.
 */
enum rfl_status rfl_network_start(struct rfl_network *n);

/**
 * Signal the network to shut down. Non-blocking.
 */
enum rfl_status rfl_network_shutdown(struct rfl_network *n);

/**
 * Free a network handle. Safe on NULL. Implies shutdown.
 */
void rfl_network_free(struct rfl_network *n);

/**
 * Subscribe to the network's event stream. Call **before**
 * `rfl_network_start` for full coverage.
 */
struct rfl_events *rfl_network_events(struct rfl_network *n);

/**
 * Poll for the next event, blocking up to `timeout_ms` milliseconds.
 *
 * On success, writes a newly allocated JSON-encoded event string to
 * `*out_json`. Caller frees with `rfl_string_free`.
 *
 * Returns:
 * - `rfl_status::Ok` on success.
 * - `rfl_status::Runtime` if the channel is closed (network dropped).
 * - `rfl_status::InvalidState` on timeout (no event, channel still open).
 */
enum rfl_status rfl_events_recv(struct rfl_events *e, uint32_t timeout_ms, char **out_json);

/**
 * Free an events handle. Safe on NULL.
 */
void rfl_events_free(struct rfl_events *e);

/**
 * Runtime version string (newly allocated; free with `rfl_string_free`).
 */
char *rfl_version(void);

/**
 * Add a node.
 * `metadata_json` may be NULL or a JSON object string (`{"key": ...}`).
 */
enum rfl_status rfl_graph_add_node(struct rfl_graph *g,
                                   const char *id,
                                   const char *component,
                                   const char *metadata_json);

enum rfl_status rfl_graph_remove_node(struct rfl_graph *g, const char *id);

/**
 * Replace the metadata for an existing node.
 */
enum rfl_status rfl_graph_set_node_metadata(struct rfl_graph *g,
                                            const char *id,
                                            const char *metadata_json);

enum rfl_status rfl_graph_add_connection(struct rfl_graph *g,
                                         const char *out_node,
                                         const char *out_port,
                                         const char *in_node,
                                         const char *in_port,
                                         const char *metadata_json);

enum rfl_status rfl_graph_remove_connection(struct rfl_graph *g,
                                            const char *out_node,
                                            const char *out_port,
                                            const char *in_node,
                                            const char *in_port);

/**
 * Add an initial packet. `data_json` is the JSON representation of the
 * value (not a `Message`) — matches `Graph::add_initial`.
 */
enum rfl_status rfl_graph_add_initial(struct rfl_graph *g,
                                      const char *node,
                                      const char *port,
                                      const char *data_json,
                                      const char *metadata_json);

enum rfl_status rfl_graph_remove_initial(struct rfl_graph *g, const char *node, const char *port);

/**
 * Expose an inport on the graph (used when this graph is embedded as a
 * subgraph). `port_type_json` is optional; pass NULL for `PortType::Any`
 * or `"\"All\""` / `"\"Flow\""` etc.
 */
enum rfl_status rfl_graph_add_inport(struct rfl_graph *g,
                                     const char *port_id,
                                     const char *node_id,
                                     const char *port_key,
                                     const char *port_type_json,
                                     const char *metadata_json);

enum rfl_status rfl_graph_add_outport(struct rfl_graph *g,
                                      const char *port_id,
                                      const char *node_id,
                                      const char *port_key,
                                      const char *port_type_json,
                                      const char *metadata_json);

enum rfl_status rfl_graph_remove_inport(struct rfl_graph *g, const char *port_id);

enum rfl_status rfl_graph_remove_outport(struct rfl_graph *g, const char *port_id);

/**
 * Serialize a graph back to its `GraphExport` JSON form.
 * Caller frees via `rfl_string_free`.
 */
char *rfl_graph_to_json(struct rfl_graph *g);

/**
 * Add a node to a running or pending network. The `template_id`'s actor
 * must have been registered first (via the component catalog or a custom
 * `rfl_actor_register` once available).
 */
enum rfl_status rfl_network_add_node(struct rfl_network *n,
                                     const char *id,
                                     const char *template_id,
                                     const char *config_json);

enum rfl_status rfl_network_add_connection(struct rfl_network *n,
                                           const char *from_actor,
                                           const char *from_port,
                                           const char *to_actor,
                                           const char *to_port);

/**
 * Seed an initial packet. `message_json` must parse as a `Message`
 * (e.g. `"\"Flow\""`, `{"Integer": 3}`, `{"String": "hi"}`).
 */
enum rfl_status rfl_network_add_initial(struct rfl_network *n,
                                        const char *actor,
                                        const char *port,
                                        const char *message_json);

/**
 * Create a callback-driven actor.
 *
 * `inports` / `outports` arrays point to `n_*` C strings each (UTF-8).
 * `await_all_inports` = 1 makes the runtime buffer packets until every
 * declared inport has data; 0 fires on any input.
 *
 * `user_data_drop` may be NULL if no cleanup is needed.
 */
struct rfl_actor *rfl_actor_new(const char *component_name,
                                const char *const *inports,
                                uintptr_t n_inports,
                                const char *const *outports,
                                uintptr_t n_outports,
                                int await_all_inports,
                                rfl_actor_fn callback,
                                void *user_data,
                                rfl_actor_drop_fn user_data_drop);

/**
 * Free an actor handle that was never registered. NULL-safe.
 */
void rfl_actor_free(struct rfl_actor *a);

/**
 * Register a callback actor as a template on the network. The network
 * takes ownership; the caller must **not** continue to use the handle
 * afterwards (treat it as freed).
 */
enum rfl_status rfl_network_register_actor(struct rfl_network *n,
                                           const char *template_id,
                                           struct rfl_actor *a);

/**
 * 1 if a packet is available on `port`, 0 otherwise. Does not consume.
 */
int rfl_ctx_has_input(struct rfl_actor_ctx *ctx, const char *port);

/**
 * Take the input packet on `port` as a typed message handle, removing
 * it from the context. Returns NULL if no packet is available. Caller
 * frees via `rfl_message_free` (or transfers ownership via
 * `rfl_ctx_emit_message`).
 */
struct rfl_message *rfl_ctx_take_input_message(struct rfl_actor_ctx *ctx, const char *port);

/**
 * Return the input packet on `port` as a JSON-encoded `Message`, or NULL
 * if no packet is available. Caller frees via `rfl_string_free`.
 */
char *rfl_ctx_input_json(struct rfl_actor_ctx *ctx, const char *port);

/**
 * Return the current config as a JSON object string. Caller frees via
 * `rfl_string_free`. Returns NULL on error.
 */
char *rfl_ctx_config_json(struct rfl_actor_ctx *ctx);

/**
 * Fetch a state entry as JSON. Returns NULL if absent.
 * Caller frees via `rfl_string_free`.
 *
 * Only works against `MemoryState` (the default state backend) — custom
 * backends will yield NULL.
 */
char *rfl_ctx_state_get(struct rfl_actor_ctx *ctx, const char *key);

/**
 * Set a state entry. `value_json` is a JSON value (any shape).
 */
enum rfl_status rfl_ctx_state_set(struct rfl_actor_ctx *ctx,
                                  const char *key,
                                  const char *value_json);

/**
 * Emit a typed message on `port`. Transfers ownership of the message —
 * do **not** call `rfl_message_free` afterwards. Prefer this over the
 * JSON variant for hot-path emits.
 */
enum rfl_status rfl_ctx_emit_message(struct rfl_actor_ctx *ctx,
                                     const char *port,
                                     struct rfl_message *msg);

/**
 * Emit a packet on `port`. `message_json` must parse as a Reflow
 * `Message` (e.g. `{"type":"Flow"}`, `{"type":"Integer","data":1}`).
 * Prefer `rfl_ctx_emit_message` for hot-path emits — this variant
 * serializes JSON per call.
 */
enum rfl_status rfl_ctx_emit(struct rfl_actor_ctx *ctx, const char *port, const char *message_json);

struct rfl_message *rfl_message_flow(void);

struct rfl_message *rfl_message_boolean(int v);

struct rfl_message *rfl_message_integer(int64_t v);

struct rfl_message *rfl_message_float(double v);

/**
 * UTF-8 string; copied.
 */
struct rfl_message *rfl_message_string(const char *s);

/**
 * Binary payload; the buffer is copied into a refcounted allocation.
 */
struct rfl_message *rfl_message_bytes(const uint8_t *data, uintptr_t len);

/**
 * Object from JSON. The JSON must parse as any valid serde value.
 */
struct rfl_message *rfl_message_object_from_json(const char *json);

/**
 * Array from a JSON array string.
 */
struct rfl_message *rfl_message_array_from_json(const char *json);

struct rfl_message *rfl_message_error(const char *msg);

/**
 * Fallback: parse a fully-tagged `Message` JSON (i.e. the same shape the
 * legacy `rfl_ctx_emit`-by-JSON path consumes). Useful for tests /
 * debugging — prefer the typed constructors in production.
 */
struct rfl_message *rfl_message_from_json(const char *json);

enum rfl_message_kind rfl_message_get_kind(const struct rfl_message *m);

/**
 * If the message is a Boolean, writes its value into `*out` and returns 1.
 * Returns 0 otherwise.
 */
int rfl_message_as_boolean(const struct rfl_message *m, int *out);

int rfl_message_as_integer(const struct rfl_message *m, int64_t *out);

int rfl_message_as_float(const struct rfl_message *m, double *out);

/**
 * String access — returns a newly allocated C string on String/Error
 * variants, else NULL. Caller frees via `rfl_string_free`.
 */
char *rfl_message_as_string(const struct rfl_message *m);

/**
 * Full message serialized as JSON. Always succeeds for representable
 * variants. Caller frees via `rfl_string_free`.
 */
char *rfl_message_as_json(const struct rfl_message *m);

/**
 * Zero-copy borrow of a `Bytes` payload. Writes a pointer into the
 * Arc'd buffer and its length. The pointer is valid until the message
 * handle is freed (or ownership transferred). Returns 1 on success, 0
 * if the message is not a Bytes variant.
 */
int rfl_message_bytes_borrow(const struct rfl_message *m,
                             const uint8_t **out_data,
                             uintptr_t *out_len);

/**
 * Free a message handle. Safe on NULL.
 *
 * Do **not** call this after handing the message to `rfl_ctx_emit`
 * (which transfers ownership).
 */
void rfl_message_free(struct rfl_message *m);

/**
 * Allocate a new stream. `buffer_size == 0` creates an unbounded channel;
 * any positive value sets a bounded buffer (backpressure).
 *
 * `origin_actor` and `origin_port` are metadata attached to the
 * StreamHandle that lets consumers trace where the stream came from.
 * Either may be NULL to use empty strings.
 *
 * `content_type` is an optional MIME hint (NULL for none).
 */
struct rfl_stream *rfl_stream_new(uintptr_t buffer_size,
                                  const char *origin_actor,
                                  const char *origin_port,
                                  const char *content_type);

/**
 * Send a Data frame. The buffer is copied into a refcounted allocation.
 */
enum rfl_status rfl_stream_send_bytes(struct rfl_stream *s, const uint8_t *data, uintptr_t len);

/**
 * Send a Begin frame (stream metadata). Optional — use before the first
 * `send_bytes` if you want consumers to see `content_type` / `size_hint`
 * / `metadata_json` before any data.
 */
enum rfl_status rfl_stream_send_begin(struct rfl_stream *s,
                                      const char *content_type,
                                      uint64_t size_hint,
                                      int has_size_hint,
                                      const char *metadata_json);

/**
 * Terminate the stream with success.
 */
enum rfl_status rfl_stream_end(struct rfl_stream *s);

/**
 * Terminate the stream with an error.
 */
enum rfl_status rfl_stream_error(struct rfl_stream *s, const char *message);

/**
 * Convert this stream producer into a `Message::StreamHandle` that can be
 * emitted on an output port. The producer is **consumed** — free is not
 * necessary after this call.
 */
struct rfl_message *rfl_stream_into_message(struct rfl_stream *s);

/**
 * Free a producer handle without emitting it as a message. If the stream
 * has live consumers, they will see an `End` frame when the sender is
 * dropped.
 */
void rfl_stream_free(struct rfl_stream *s);

/**
 * Take the receiver for a `StreamHandle` message. Transfers ownership —
 * only one call succeeds per stream. Returns NULL if the message is not
 * a StreamHandle or the receiver has already been taken.
 */
struct rfl_stream_recv *rfl_message_stream_take(struct rfl_message *m);

/**
 * Block up to `timeout_ms` for the next frame.
 *
 * Writes the frame kind into `*out_kind`. On `Data` / `Error`, also
 * populates `*out_data` / `*out_len` / `*out_err` as appropriate. The
 * pointers are valid until the next call to `rfl_stream_recv_next` on
 * the same receiver.
 */
enum rfl_status rfl_stream_recv_next(struct rfl_stream_recv *r,
                                     uint32_t timeout_ms,
                                     enum rfl_stream_frame_kind *out_kind,
                                     const uint8_t **out_data,
                                     uintptr_t *out_len,
                                     char **out_err);

/**
 * Free a stream receiver. Safe on NULL.
 */
void rfl_stream_recv_free(struct rfl_stream_recv *r);

/**
 * Start a subgraph builder over a `GraphExport` JSON document. Returns
 * NULL on parse error.
 */
struct rfl_subgraph_builder *rfl_subgraph_builder_new(const char *graph_export_json);

/**
 * Register an actor under `component_name`. The builder takes ownership
 * of the actor handle — do not free it afterwards. Replacing an existing
 * registration silently overwrites it.
 */
enum rfl_status rfl_subgraph_builder_register_actor(struct rfl_subgraph_builder *b,
                                                    const char *component_name,
                                                    struct rfl_actor *actor);

/**
 * Resolve any still-unregistered components from the bundled catalog.
 * Gated on the `components` feature; no-op (returns Ok) otherwise.
 */
enum rfl_status rfl_subgraph_builder_fill_from_catalog(struct rfl_subgraph_builder *b);

/**
 * Build the subgraph into an actor handle. Consumes the builder.
 */
struct rfl_actor *rfl_subgraph_builder_build(struct rfl_subgraph_builder *b);

/**
 * Abandon a builder without building. Safe on NULL.
 */
void rfl_subgraph_builder_free(struct rfl_subgraph_builder *b);

/**
 * Compose N graph exports into a single `GraphExport` JSON document.
 *
 * The returned string is a heap-allocated C string owned by the caller;
 * free with `rfl_string_free`. Returns NULL on failure; read the error
 * message via `rfl_last_error_message`.
 */
char *rfl_compose_graphs(const char *composition_json);

/**
 * Instantiate an actor from the bundled `reflow_components` catalog.
 *
 * Returns an `rfl_actor*` that can be handed to `rfl_network_register_actor`
 * exactly like a callback-driven actor. Returns NULL if the template id is
 * not recognised.
 *
 * Available only when the crate is compiled with the `components` feature
 * (on by default).
 */
struct rfl_actor *rfl_template_actor_new(const char *template_id);

/**
 * Return a JSON array of every template id the bundled catalog knows
 * about. Caller frees via `rfl_string_free`.
 */
char *rfl_template_list_json(void);

/**
 * Build a SubgraphActor from a `GraphExport` JSON document. Each component
 * referenced inside the export is resolved against the bundled
 * `reflow_components` catalog. Returns NULL on parse error or on unknown
 * component references.
 *
 * The resulting `rfl_actor*` can be handed to `rfl_network_register_actor`
 * exactly like any other actor template.
 */
struct rfl_actor *rfl_subgraph_actor_new_from_json(const char *graph_export_json);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* REFLOW_RT_H */
