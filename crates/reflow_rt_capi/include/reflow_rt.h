#ifndef REFLOW_RT_H
#define REFLOW_RT_H

/* GENERATED FILE — edit src/lib.rs and rerun `cargo build --features generate-header`. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef enum rfl_status {
  /**
   * Success.
   */
  Ok = 0,
  /**
   * A required argument was NULL.
   */
  NullArg = -1,
  /**
   * A C string was not valid UTF-8.
   */
  InvalidUtf8 = -2,
  /**
   * A JSON payload failed to parse or deserialize.
   */
  InvalidJson = -3,
  /**
   * The runtime refused the operation (see `rfl_last_error_message`).
   */
  Runtime = -4,
  /**
   * The network has already been started or is in a state that forbids
   * the operation.
   */
  InvalidState = -5,
} rfl_status;

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
 * Opaque handle to a `reflow_network::Network`, wrapped so the C side can
 * share it across threads without touching its internal `Arc<Mutex<_>>`.
 */
typedef struct rfl_network rfl_network;

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
 * Create a new network. Currently uses `NetworkConfig::default()`.
 */
struct rfl_network *rfl_network_new(void);

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
 * Emit a packet on `port`. `message_json` must parse as a Reflow
 * `Message` (e.g. `"\"Flow\""`, `{"Integer": 1}`, `{"String": "x"}`).
 */
enum rfl_status rfl_ctx_emit(struct rfl_actor_ctx *ctx, const char *port, const char *message_json);

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
