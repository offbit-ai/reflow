// Reflow — C++17 header-only wrapper over the C ABI.
//
//   #include <reflow/reflow.hpp>
//   #include <reflow/reflow_rt.h>   // ← provided in the same dir; link
//                                     against `libreflow_rt_capi`.
//
// Everything throws `reflow::Error` on a non-OK status from the C API.
// Handle ownership is RAII: every wrapper class owns its `rfl_*` pointer
// via a unique_ptr with a custom deleter. Strings returned from the C
// side (`char*` allocated by the runtime, freed via `rfl_string_free`)
// are converted to `std::string` immediately so callers never deal with
// raw lifetime.
//
// The JSON-shaped return values (`get_node_json`, `connections_json`,
// etc.) are handed back as `std::string`. Pick your own JSON library
// (nlohmann/json, simdjson, …) to parse them.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

#pragma once

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "reflow_rt.h"

namespace reflow {

// ─── error type ────────────────────────────────────────────────────────────

class Error : public std::runtime_error {
public:
    Error(rfl_status status, std::string message)
        : std::runtime_error(std::move(message)), status_(status) {}

    rfl_status status() const noexcept { return status_; }

private:
    rfl_status status_;
};

namespace detail {

inline std::string take_c_string(char* p) {
    if (p == nullptr) return {};
    std::string out(p);
    rfl_string_free(p);
    return out;
}

inline std::string last_error_message() {
    char* msg = rfl_last_error_message();
    if (msg == nullptr) return "(no error)";
    return take_c_string(msg);
}

[[noreturn]] inline void throw_status(rfl_status s, std::string_view op) {
    throw Error(s, std::string(op) + ": " + last_error_message());
}

inline void check(rfl_status s, std::string_view op) {
    if (s != rfl_status_Ok) throw_status(s, op);
}

// Wrap a `char*` that the C API allocated; throw if NULL.
inline std::string take_or_throw(char* p, std::string_view op) {
    if (p == nullptr) throw_status(rfl_status_Runtime, op);
    return take_c_string(p);
}

// Wrap a `char*` that may legitimately be NULL (e.g. "no such node").
// Distinguish "miss" from "error" by inspecting the thread-local last
// error: a real error sets it; a miss leaves it untouched (the C side
// clears it at the start of every call, then sets it only on failure).
inline std::optional<std::string> take_optional(char* p, std::string_view op) {
    if (p != nullptr) return take_c_string(p);
    char* msg = rfl_last_error_message();
    if (msg == nullptr) return std::nullopt;
    std::string m = take_c_string(msg);
    throw Error(rfl_status_Runtime, std::string(op) + ": " + m);
}

inline const char* c(const std::string& s) { return s.c_str(); }
inline const char* c_or_null(const std::optional<std::string>& s) {
    return s ? s->c_str() : nullptr;
}

template <typename Handle, void (*Free)(Handle*)>
struct HandleDeleter {
    void operator()(Handle* h) const noexcept {
        if (h != nullptr) Free(h);
    }
};

}  // namespace detail

template <typename Handle, void (*Free)(Handle*)>
using UniqueHandle = std::unique_ptr<Handle, detail::HandleDeleter<Handle, Free>>;

// ─── runtime / version ─────────────────────────────────────────────────────

inline std::string version() {
    return detail::take_or_throw(rfl_version(), "version");
}

inline void shutdown() noexcept { rfl_runtime_shutdown(); }

// ─── Message ───────────────────────────────────────────────────────────────

class Message {
public:
    explicit Message(rfl_message* m) : ptr_(m) {}

    static Message from_json(std::string_view json) {
        rfl_message* m = rfl_message_from_json(json.data());
        if (m == nullptr) detail::throw_status(rfl_status_InvalidJson, "Message::from_json");
        return Message(m);
    }

    static Message flow()                                    { return Message(rfl_message_flow()); }
    static Message boolean(bool v)                           { return Message(rfl_message_boolean(v ? 1 : 0)); }
    static Message integer(int64_t v)                        { return Message(rfl_message_integer(v)); }
    static Message floating(double v)                        { return Message(rfl_message_float(v)); }
    static Message string(std::string_view v)                { return Message(rfl_message_string(v.data())); }
    static Message bytes(const uint8_t* data, size_t n)      { return Message(rfl_message_bytes(data, n)); }

    rfl_message_kind kind() const noexcept { return rfl_message_get_kind(ptr_.get()); }

    std::string as_json() const {
        return detail::take_or_throw(rfl_message_as_json(ptr_.get()), "Message::as_json");
    }

    rfl_message* raw() const noexcept { return ptr_.get(); }
    rfl_message* release() noexcept { return ptr_.release(); }

private:
    UniqueHandle<rfl_message, rfl_message_free> ptr_;
};

// ─── Graph ─────────────────────────────────────────────────────────────────

class Graph {
public:
    Graph() : Graph("", false) {}

    explicit Graph(std::string_view name, bool case_sensitive = false) {
        ptr_.reset(rfl_graph_new(std::string(name).c_str(), case_sensitive ? 1 : 0));
        if (!ptr_) detail::throw_status(rfl_status_Runtime, "Graph::Graph");
    }

    static Graph from_json(std::string_view json) {
        Graph g(nullptr);
        g.ptr_.reset(rfl_graph_load_json(std::string(json).c_str()));
        if (!g.ptr_) detail::throw_status(rfl_status_InvalidJson, "Graph::from_json");
        return g;
    }

    std::string to_json() const {
        return detail::take_or_throw(rfl_graph_to_json(ptr_.get()), "Graph::to_json");
    }

    // ── mutators (existing) ───────────────────────────────────────────

    void add_node(std::string_view id, std::string_view component,
                  const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(
            rfl_graph_add_node(ptr_.get(), std::string(id).c_str(),
                               std::string(component).c_str(),
                               detail::c_or_null(metadata_json)),
            "Graph::add_node");
    }

    void remove_node(std::string_view id) {
        detail::check(
            rfl_graph_remove_node(ptr_.get(), std::string(id).c_str()),
            "Graph::remove_node");
    }

    void add_connection(std::string_view out_node, std::string_view out_port,
                        std::string_view in_node, std::string_view in_port,
                        const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(
            rfl_graph_add_connection(
                ptr_.get(),
                std::string(out_node).c_str(), std::string(out_port).c_str(),
                std::string(in_node).c_str(), std::string(in_port).c_str(),
                detail::c_or_null(metadata_json)),
            "Graph::add_connection");
    }

    void remove_connection(std::string_view out_node, std::string_view out_port,
                           std::string_view in_node, std::string_view in_port) {
        detail::check(
            rfl_graph_remove_connection(
                ptr_.get(),
                std::string(out_node).c_str(), std::string(out_port).c_str(),
                std::string(in_node).c_str(), std::string(in_port).c_str()),
            "Graph::remove_connection");
    }

    void add_initial(std::string_view node, std::string_view port,
                     std::string_view data_json,
                     const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(
            rfl_graph_add_initial(ptr_.get(),
                                  std::string(node).c_str(),
                                  std::string(port).c_str(),
                                  std::string(data_json).c_str(),
                                  detail::c_or_null(metadata_json)),
            "Graph::add_initial");
    }

    void remove_initial(std::string_view node, std::string_view port) {
        detail::check(
            rfl_graph_remove_initial(ptr_.get(),
                                     std::string(node).c_str(),
                                     std::string(port).c_str()),
            "Graph::remove_initial");
    }

    // ── mutators (renames) ────────────────────────────────────────────

    void rename_node(std::string_view old_id, std::string_view new_id) {
        detail::check(rfl_graph_rename_node(ptr_.get(),
                                            std::string(old_id).c_str(),
                                            std::string(new_id).c_str()),
                      "Graph::rename_node");
    }

    void rename_inport(std::string_view old_port, std::string_view new_port) {
        detail::check(rfl_graph_rename_inport(ptr_.get(),
                                              std::string(old_port).c_str(),
                                              std::string(new_port).c_str()),
                      "Graph::rename_inport");
    }

    void rename_outport(std::string_view old_port, std::string_view new_port) {
        detail::check(rfl_graph_rename_outport(ptr_.get(),
                                               std::string(old_port).c_str(),
                                               std::string(new_port).c_str()),
                      "Graph::rename_outport");
    }

    // ── mutators (port lifecycle) ─────────────────────────────────────

    void add_inport(std::string_view port_id, std::string_view node_id,
                    std::string_view port_key,
                    const std::optional<std::string>& port_type_json = std::nullopt,
                    const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_inport(ptr_.get(),
                                           std::string(port_id).c_str(),
                                           std::string(node_id).c_str(),
                                           std::string(port_key).c_str(),
                                           detail::c_or_null(port_type_json),
                                           detail::c_or_null(metadata_json)),
                      "Graph::add_inport");
    }

    void add_outport(std::string_view port_id, std::string_view node_id,
                     std::string_view port_key,
                     const std::optional<std::string>& port_type_json = std::nullopt,
                     const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_outport(ptr_.get(),
                                            std::string(port_id).c_str(),
                                            std::string(node_id).c_str(),
                                            std::string(port_key).c_str(),
                                            detail::c_or_null(port_type_json),
                                            detail::c_or_null(metadata_json)),
                      "Graph::add_outport");
    }

    void remove_inport(std::string_view port_id) {
        detail::check(rfl_graph_remove_inport(ptr_.get(), std::string(port_id).c_str()),
                      "Graph::remove_inport");
    }

    void remove_outport(std::string_view port_id) {
        detail::check(rfl_graph_remove_outport(ptr_.get(), std::string(port_id).c_str()),
                      "Graph::remove_outport");
    }

    // ── mutators (groups) ─────────────────────────────────────────────

    /// `nodes_json` must be a JSON array of strings.
    void add_group(std::string_view group_id, std::string_view nodes_json,
                   const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_group(ptr_.get(),
                                          std::string(group_id).c_str(),
                                          std::string(nodes_json).c_str(),
                                          detail::c_or_null(metadata_json)),
                      "Graph::add_group");
    }

    void remove_group(std::string_view group_id) {
        detail::check(rfl_graph_remove_group(ptr_.get(), std::string(group_id).c_str()),
                      "Graph::remove_group");
    }

    void add_to_group(std::string_view group_id, std::string_view node_id) {
        detail::check(rfl_graph_add_to_group(ptr_.get(),
                                             std::string(group_id).c_str(),
                                             std::string(node_id).c_str()),
                      "Graph::add_to_group");
    }

    void remove_from_group(std::string_view group_id, std::string_view node_id) {
        detail::check(rfl_graph_remove_from_group(ptr_.get(),
                                                  std::string(group_id).c_str(),
                                                  std::string(node_id).c_str()),
                      "Graph::remove_from_group");
    }

    // ── mutators (initial-packet variants) ────────────────────────────

    void add_initial_index(std::string_view node, std::string_view port,
                           std::string_view data_json, size_t index,
                           const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_initial_index(ptr_.get(),
                                                  std::string(node).c_str(),
                                                  std::string(port).c_str(),
                                                  std::string(data_json).c_str(),
                                                  index,
                                                  detail::c_or_null(metadata_json)),
                      "Graph::add_initial_index");
    }

    void add_graph_initial(std::string_view inport, std::string_view data_json,
                           const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_graph_initial(ptr_.get(),
                                                  std::string(inport).c_str(),
                                                  std::string(data_json).c_str(),
                                                  detail::c_or_null(metadata_json)),
                      "Graph::add_graph_initial");
    }

    void add_graph_initial_index(std::string_view inport, std::string_view data_json,
                                 size_t index,
                                 const std::optional<std::string>& metadata_json = std::nullopt) {
        detail::check(rfl_graph_add_graph_initial_index(ptr_.get(),
                                                        std::string(inport).c_str(),
                                                        std::string(data_json).c_str(),
                                                        index,
                                                        detail::c_or_null(metadata_json)),
                      "Graph::add_graph_initial_index");
    }

    void remove_graph_initial(std::string_view inport) {
        detail::check(rfl_graph_remove_graph_initial(ptr_.get(),
                                                     std::string(inport).c_str()),
                      "Graph::remove_graph_initial");
    }

    // ── mutators (metadata setters + properties) ──────────────────────

    void set_node_metadata(std::string_view id, std::string_view metadata_json) {
        detail::check(rfl_graph_set_node_metadata(ptr_.get(),
                                                  std::string(id).c_str(),
                                                  std::string(metadata_json).c_str()),
                      "Graph::set_node_metadata");
    }

    void set_connection_metadata(std::string_view out_node, std::string_view out_port,
                                 std::string_view in_node, std::string_view in_port,
                                 std::string_view metadata_json) {
        detail::check(rfl_graph_set_connection_metadata(
                          ptr_.get(),
                          std::string(out_node).c_str(),
                          std::string(out_port).c_str(),
                          std::string(in_node).c_str(),
                          std::string(in_port).c_str(),
                          std::string(metadata_json).c_str()),
                      "Graph::set_connection_metadata");
    }

    void set_inport_metadata(std::string_view port_id, std::string_view metadata_json) {
        detail::check(rfl_graph_set_inport_metadata(ptr_.get(),
                                                    std::string(port_id).c_str(),
                                                    std::string(metadata_json).c_str()),
                      "Graph::set_inport_metadata");
    }

    void set_outport_metadata(std::string_view port_id, std::string_view metadata_json) {
        detail::check(rfl_graph_set_outport_metadata(ptr_.get(),
                                                     std::string(port_id).c_str(),
                                                     std::string(metadata_json).c_str()),
                      "Graph::set_outport_metadata");
    }

    void set_group_metadata(std::string_view group_id, std::string_view metadata_json) {
        detail::check(rfl_graph_set_group_metadata(ptr_.get(),
                                                   std::string(group_id).c_str(),
                                                   std::string(metadata_json).c_str()),
                      "Graph::set_group_metadata");
    }

    void set_properties(std::string_view properties_json) {
        detail::check(rfl_graph_set_properties(ptr_.get(),
                                               std::string(properties_json).c_str()),
                      "Graph::set_properties");
    }

    /// Replace this graph's contents with another GraphExport (destructive).
    void import(std::string_view export_json) {
        detail::check(rfl_graph_import(ptr_.get(), std::string(export_json).c_str()),
                      "Graph::import");
    }

    // ── queries (return JSON or std::nullopt for "not found") ─────────

    std::optional<std::string> get_node_json(std::string_view id) const {
        return detail::take_optional(rfl_graph_get_node_json(ptr_.get(), std::string(id).c_str()),
                                     "Graph::get_node_json");
    }

    std::string nodes_json() const {
        return detail::take_or_throw(rfl_graph_list_nodes_json(ptr_.get()),
                                     "Graph::nodes_json");
    }

    std::optional<std::string> get_connection_json(std::string_view out_node,
                                                   std::string_view out_port,
                                                   std::string_view in_node,
                                                   std::string_view in_port) const {
        return detail::take_optional(
            rfl_graph_get_connection_json(ptr_.get(),
                                          std::string(out_node).c_str(),
                                          std::string(out_port).c_str(),
                                          std::string(in_node).c_str(),
                                          std::string(in_port).c_str()),
            "Graph::get_connection_json");
    }

    std::string connections_json() const {
        return detail::take_or_throw(rfl_graph_list_connections_json(ptr_.get()),
                                     "Graph::connections_json");
    }

    std::string groups_json() const {
        return detail::take_or_throw(rfl_graph_list_groups_json(ptr_.get()),
                                     "Graph::groups_json");
    }

    std::string inports_json() const {
        return detail::take_or_throw(rfl_graph_list_inports_json(ptr_.get()),
                                     "Graph::inports_json");
    }

    std::string outports_json() const {
        return detail::take_or_throw(rfl_graph_list_outports_json(ptr_.get()),
                                     "Graph::outports_json");
    }

    std::string initializers_json() const {
        return detail::take_or_throw(rfl_graph_list_initializers_json(ptr_.get()),
                                     "Graph::initializers_json");
    }

    std::string properties_json() const {
        return detail::take_or_throw(rfl_graph_get_properties_json(ptr_.get()),
                                     "Graph::properties_json");
    }

    rfl_graph* raw() const noexcept { return ptr_.get(); }

    /// Transfer ownership of the underlying handle. Used by
    /// `Network::from_graph` to hand the graph over to the runtime.
    rfl_graph* release() noexcept { return ptr_.release(); }

private:
    explicit Graph(std::nullptr_t) {}
    UniqueHandle<rfl_graph, rfl_graph_free> ptr_;
};

// ─── Actor context (per-tick view) ─────────────────────────────────────────

/// Thin view over `rfl_actor_ctx*`. Lives only for the duration of a
/// single behavior tick — do NOT capture it.
class Context {
public:
    explicit Context(rfl_actor_ctx* ctx) noexcept : ctx_(ctx) {}

    bool has_input(std::string_view port) const noexcept {
        return rfl_ctx_has_input(ctx_, std::string(port).c_str()) != 0;
    }

    /// Returns the input on `port` as JSON (the same shape `Message::as_json`
    /// produces), or `std::nullopt` if no value arrived this tick.
    std::optional<std::string> input_json(std::string_view port) const {
        char* p = rfl_ctx_input_json(ctx_, std::string(port).c_str());
        if (p == nullptr) return std::nullopt;
        return detail::take_c_string(p);
    }

    /// Take the input on `port` as a Message. Returns nullopt on miss.
    std::optional<Message> take_input(std::string_view port) {
        rfl_message* m = rfl_ctx_take_input_message(ctx_, std::string(port).c_str());
        if (m == nullptr) return std::nullopt;
        return Message(m);
    }

    std::string config_json() const {
        char* p = rfl_ctx_config_json(ctx_);
        return p == nullptr ? std::string{} : detail::take_c_string(p);
    }

    std::optional<std::string> state_get(std::string_view key) const {
        char* p = rfl_ctx_state_get(ctx_, std::string(key).c_str());
        if (p == nullptr) return std::nullopt;
        return detail::take_c_string(p);
    }

    void state_set(std::string_view key, std::string_view value_json) {
        detail::check(rfl_ctx_state_set(ctx_,
                                        std::string(key).c_str(),
                                        std::string(value_json).c_str()),
                      "Context::state_set");
    }

    /// Emit a message on `port`. Consumes the Message handle.
    void emit(std::string_view port, Message&& msg) {
        detail::check(
            rfl_ctx_emit_message(ctx_, std::string(port).c_str(), msg.release()),
            "Context::emit");
    }

    /// Emit a JSON-serialized message — convenience for tests where you
    /// don't already have a Message instance.
    void emit_json(std::string_view port, std::string_view message_json) {
        detail::check(rfl_ctx_emit(ctx_,
                                   std::string(port).c_str(),
                                   std::string(message_json).c_str()),
                      "Context::emit_json");
    }

    rfl_actor_ctx* raw() const noexcept { return ctx_; }

private:
    rfl_actor_ctx* ctx_;
};

// ─── Actor ─────────────────────────────────────────────────────────────────

class Actor {
public:
    explicit Actor(rfl_actor* a) : ptr_(a) {}

    /// Look up a bundled (or pack-loaded) template by id.
    /// Throws if no such template id is known.
    static Actor from_template(std::string_view template_id) {
        rfl_actor* a = rfl_template_actor_new(std::string(template_id).c_str());
        if (a == nullptr) detail::throw_status(rfl_status_Runtime, "Actor::from_template");
        return Actor(a);
    }

    /// Build an actor from a C++ behavior. The callback runs on the
    /// runtime's tokio worker threads. Holds a reference to a heap-
    /// allocated user_data lambda; the runtime calls drop on shutdown.
    /// User exceptions are caught and reported as a behavior error so
    /// the runtime sees a clean status, never a stack-unwinding edge.
    using BehaviorFn = std::function<void(Context&)>;

    static Actor from_callback(std::string_view component_name,
                               const std::vector<std::string>& inports,
                               const std::vector<std::string>& outports,
                               BehaviorFn behavior,
                               bool await_all_inports = false) {
        struct Ctx { BehaviorFn fn; };
        auto* ctx = new Ctx{std::move(behavior)};

        std::vector<const char*> in_ptrs, out_ptrs;
        for (auto& s : inports) in_ptrs.push_back(s.c_str());
        for (auto& s : outports) out_ptrs.push_back(s.c_str());

        auto thunk = [](void* ud, rfl_actor_ctx* c) -> rfl_status {
            try {
                Context view(c);
                static_cast<Ctx*>(ud)->fn(view);
                return rfl_status_Ok;
            } catch (...) {
                return rfl_status_Runtime;
            }
        };
        auto drop = [](void* ud) {
            delete static_cast<Ctx*>(ud);
        };

        rfl_actor* a = rfl_actor_new(
            std::string(component_name).c_str(),
            in_ptrs.empty() ? nullptr : in_ptrs.data(), in_ptrs.size(),
            out_ptrs.empty() ? nullptr : out_ptrs.data(), out_ptrs.size(),
            await_all_inports ? 1 : 0,
            thunk,
            ctx,
            drop);
        if (a == nullptr) {
            delete ctx;
            detail::throw_status(rfl_status_Runtime, "Actor::from_callback");
        }
        return Actor(a);
    }

    rfl_actor* raw() const noexcept { return ptr_.get(); }

    /// Transfer ownership of the underlying handle. The Actor object is
    /// left empty after; intended for `Network::register_actor` which
    /// consumes the pointer.
    rfl_actor* release() noexcept { return ptr_.release(); }

private:
    UniqueHandle<rfl_actor, rfl_actor_free> ptr_;
};

// ─── EventStream ───────────────────────────────────────────────────────────

class EventStream {
public:
    explicit EventStream(rfl_events* e) : ptr_(e) {
        if (!ptr_) detail::throw_status(rfl_status_Runtime, "EventStream");
    }

    /// Block for up to `timeout_ms` milliseconds for the next event JSON.
    /// Returns nullopt on timeout (`rfl_events_recv` reports timeout as
    /// `rfl_status_InvalidState`) or when the event channel has been
    /// closed.
    std::optional<std::string> recv(uint32_t timeout_ms) {
        char* out = nullptr;
        rfl_status s = rfl_events_recv(ptr_.get(), timeout_ms, &out);
        if (s == rfl_status_Ok) {
            if (out == nullptr) return std::nullopt;
            return detail::take_c_string(out);
        }
        if (s == rfl_status_InvalidState) return std::nullopt;  // timeout
        if (s == rfl_status_Runtime) return std::nullopt;       // channel closed
        detail::throw_status(s, "EventStream::recv");
    }

private:
    UniqueHandle<rfl_events, rfl_events_free> ptr_;
};

// ─── Network ───────────────────────────────────────────────────────────────

class Network {
public:
    Network() : ptr_(rfl_network_new()) {
        if (!ptr_) detail::throw_status(rfl_status_Runtime, "Network::Network");
    }

    explicit Network(std::string_view config_json)
        : ptr_(rfl_network_new_with_config(std::string(config_json).c_str())) {
        if (!ptr_) detail::throw_status(rfl_status_InvalidJson, "Network::Network");
    }

    /// Build a network from a Graph. Consumes the graph handle —
    /// the C ABI takes ownership.
    static Network from_graph(Graph&& graph) {
        rfl_graph* raw = graph.release();
        rfl_network* n = rfl_network_from_graph(raw);
        if (n == nullptr) {
            // capi only takes ownership on success; on failure we
            // would normally free, but rfl_network_from_graph
            // *always* takes ownership (Box::from_raw at the top of
            // the impl), so don't double-free here.
            detail::throw_status(rfl_status_Runtime, "Network::from_graph");
        }
        Network net;
        net.ptr_.reset(n);
        return net;
    }

    void start() {
        detail::check(rfl_network_start(ptr_.get()), "Network::start");
    }

    void shutdown() {
        detail::check(rfl_network_shutdown(ptr_.get()), "Network::shutdown");
    }

    /// Subscribe to runtime events. The returned stream owns the
    /// underlying `rfl_events*` and can be polled with `recv`.
    EventStream events() {
        rfl_events* e = rfl_network_events(ptr_.get());
        if (e == nullptr) detail::throw_status(rfl_status_Runtime, "Network::events");
        return EventStream(e);
    }

    /// Register an actor under a template id. Consumes the actor handle.
    void register_actor(std::string_view template_id, Actor&& actor) {
        detail::check(rfl_network_register_actor(ptr_.get(),
                                                 std::string(template_id).c_str(),
                                                 actor.release()),
                      "Network::register_actor");
    }

    void add_node(std::string_view id, std::string_view template_id,
                  const std::optional<std::string>& config_json = std::nullopt) {
        detail::check(rfl_network_add_node(ptr_.get(),
                                           std::string(id).c_str(),
                                           std::string(template_id).c_str(),
                                           detail::c_or_null(config_json)),
                      "Network::add_node");
    }

    void add_connection(std::string_view from_actor, std::string_view from_port,
                        std::string_view to_actor, std::string_view to_port) {
        detail::check(rfl_network_add_connection(
                          ptr_.get(),
                          std::string(from_actor).c_str(), std::string(from_port).c_str(),
                          std::string(to_actor).c_str(), std::string(to_port).c_str()),
                      "Network::add_connection");
    }

    /// `message_json` is the JSON form of a `Message` (e.g.
    /// `{"type":"Integer","data":42}`). Use `Message::as_json()` to
    /// serialize an existing Message instance.
    void add_initial(std::string_view actor, std::string_view port,
                     std::string_view message_json) {
        detail::check(rfl_network_add_initial(ptr_.get(),
                                              std::string(actor).c_str(),
                                              std::string(port).c_str(),
                                              std::string(message_json).c_str()),
                      "Network::add_initial");
    }

    rfl_network* raw() const noexcept { return ptr_.get(); }

private:
    UniqueHandle<rfl_network, rfl_network_free> ptr_;
};

// ─── Subgraph builder ──────────────────────────────────────────────────────

class SubgraphBuilder {
public:
    explicit SubgraphBuilder(std::string_view export_json)
        : ptr_(rfl_subgraph_builder_new(std::string(export_json).c_str())) {
        if (!ptr_) detail::throw_status(rfl_status_InvalidJson, "SubgraphBuilder");
    }

    void register_actor(std::string_view component_name, Actor&& actor) {
        detail::check(rfl_subgraph_builder_register_actor(
                          ptr_.get(),
                          std::string(component_name).c_str(),
                          actor.release()),
                      "SubgraphBuilder::register_actor");
    }

    /// Resolve any still-unregistered components from the bundled
    /// catalog (and pack registry).
    void fill_from_catalog() {
        detail::check(rfl_subgraph_builder_fill_from_catalog(ptr_.get()),
                      "SubgraphBuilder::fill_from_catalog");
    }

    /// Build and consume the builder. Returns the subgraph as an Actor
    /// suitable for `Network::register_actor`.
    Actor build() {
        rfl_actor* a = rfl_subgraph_builder_build(ptr_.release());
        if (a == nullptr) detail::throw_status(rfl_status_Runtime, "SubgraphBuilder::build");
        return Actor(a);
    }

private:
    UniqueHandle<rfl_subgraph_builder, rfl_subgraph_builder_free> ptr_;
};

// ─── Template catalog ──────────────────────────────────────────────────────

inline std::string template_list_json() {
    return detail::take_or_throw(rfl_template_list_json(), "template_list_json");
}

// ─── Pack loader ───────────────────────────────────────────────────────────

namespace pack {

/// Load a `.rflpack` bundle or raw cdylib. Returns the JSON array of
/// templates the pack published. Idempotent per pack name.
inline std::string load(std::string_view path) {
    char* out = nullptr;
    rfl_status s = rfl_pack_load(std::string(path).c_str(), &out);
    detail::check(s, "pack::load");
    return detail::take_c_string(out);
}

inline std::string list_json() {
    return detail::take_or_throw(rfl_pack_list_json(), "pack::list_json");
}

inline std::string inspect_json(std::string_view path) {
    return detail::take_or_throw(
        rfl_pack_inspect_json(std::string(path).c_str()),
        "pack::inspect_json");
}

inline uint32_t abi_version() noexcept { return rfl_pack_abi_version(); }

}  // namespace pack

// ─── Multi-graph composition ───────────────────────────────────────────────

inline std::string compose_graphs(std::string_view request_json) {
    return detail::take_or_throw(
        rfl_compose_graphs(std::string(request_json).c_str()),
        "compose_graphs");
}

}  // namespace reflow
