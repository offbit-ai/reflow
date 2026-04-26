// Quickstart: end-to-end use of the Reflow runtime from C++.
//
// Builds a network with two custom actors (a doubler and a collector),
// wires them up, drives an initial packet through, and streams runtime
// events back out.
//
//   $ cmake -S sdk/cpp -B build -DREFLOW_CPP_BUILD_EXAMPLES=ON \
//           -DREFLOW_RT_CAPI_LIB=$PWD/target/release/libreflow_rt_capi.dylib
//   $ cmake --build build
//   $ build/reflow_cpp_quickstart

#include <reflow/reflow.hpp>

#include <atomic>
#include <chrono>
#include <iostream>
#include <thread>

namespace ch = std::chrono;

// Pull the integer payload out of a Message JSON object of the form
// `{"type":"Integer","data":N}` without bringing in a JSON library.
static std::optional<int64_t> integer_from(std::string_view json) {
    auto pos = json.find("\"data\":");
    if (pos == std::string_view::npos) return std::nullopt;
    return std::stoll(std::string(json.substr(pos + 7)));
}

int main() {
    std::cout << "reflow runtime " << reflow::version() << "\n";

    reflow::Network net;

    // ── Custom actors via C++ closures ──────────────────────────────────────

    auto doubler = reflow::Actor::from_callback(
        "doubler", /*inports=*/{"in"}, /*outports=*/{"out"},
        [](reflow::Context& ctx) {
            auto in_json = ctx.input_json("in");
            if (!in_json) return;
            if (auto n = integer_from(*in_json)) {
                ctx.emit("out", reflow::Message::integer(*n * 2));
            }
        });

    std::atomic<int64_t> received{0};
    auto collector = reflow::Actor::from_callback(
        "collector", /*inports=*/{"in"}, /*outports=*/{},
        [&received](reflow::Context& ctx) {
            if (auto m = ctx.take_input("in")) {
                if (auto n = integer_from(m->as_json())) {
                    received = *n;
                }
            }
        });

    net.register_actor("tpl_doubler", std::move(doubler));
    net.register_actor("tpl_collector", std::move(collector));

    // ── Topology ────────────────────────────────────────────────────────────

    net.add_node("a", "tpl_doubler");
    net.add_node("b", "tpl_collector");
    net.add_connection("a", "out", "b", "in");
    net.add_initial("a", "in", R"({"type":"Integer","data":21})");

    // ── Events: subscribe before start, drain after ─────────────────────────

    auto events = net.events();
    net.start();

    // Wait briefly for the pipeline to flush.
    const auto deadline = ch::steady_clock::now() + ch::seconds(2);
    while (received.load() == 0 && ch::steady_clock::now() < deadline) {
        std::this_thread::sleep_for(ch::milliseconds(20));
    }

    std::cout << "collector received: " << received.load()
              << "  (expected 42)\n";

    // Drain any events that landed during the run. recv() returns
    // nullopt when the timeout expires with no event in the window.
    std::cout << "events seen:\n";
    for (int i = 0; i < 16; ++i) {
        auto ev = events.recv(/*timeout_ms=*/50);
        if (!ev) break;
        // Trim long event payloads for the console.
        std::cout << "  " << ev->substr(0, 100)
                  << (ev->size() > 100 ? "...\n" : "\n");
    }

    // ── Side trip: Graph authoring + queries ────────────────────────────────
    //
    // Demonstrates the full graph API surface. Construct a separate
    // graph, mutate it, query the JSON shape — handy for editor /
    // tooling integrations that don't run the network.

    reflow::Graph g("authoring-demo");
    g.add_node("x", "tpl_x");
    g.add_node("y", "tpl_y");
    g.add_connection("x", "out", "y", "in");
    g.add_group("g1", R"(["x","y"])", R"({"caption":"the lot"})");
    g.rename_node("x", "alpha");

    if (auto node = g.get_node_json("alpha")) {
        std::cout << "graph: alpha node = " << *node << "\n";
    }
    std::cout << "graph: groups = " << g.groups_json() << "\n";
    std::cout << "graph: " << reflow::template_list_json().size()
              << " bytes of bundled templates\n";

    net.shutdown();
    return received.load() == 42 ? 0 : 1;
}
