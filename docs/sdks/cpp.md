# C++ SDK

Header-only C++17 RAII wrapper over `libreflow_rt_capi`. Lives at [`sdk/cpp/`](https://github.com/offbit-ai/reflow/tree/main/sdk/cpp) in the repo. No package manager today — pull the header directly + link the runtime library.

## Requirements

- C++17 compiler (clang ≥ 9, gcc ≥ 9, MSVC 2019+)
- `libreflow_rt_capi.{so,dylib,dll}` — install via either the [`sdk/go/v*` GitHub Release tarballs](https://github.com/offbit-ai/reflow/releases?q=sdk%2Fgo) (pre-built per triple) or by building from the monorepo (`cargo build -p reflow_rt_capi --release`).

## Install

```sh
# Drop the header tree under your third_party/.
git clone --branch sdk/go/v0.2.1 --depth 1 \
    https://github.com/offbit-ai/reflow third_party/reflow

# Grab the matching native lib for your platform.
VER=0.2.1
TRIPLE=aarch64-apple-darwin
curl -LO https://github.com/offbit-ai/reflow/releases/download/sdk/go/v$VER/reflow-rt-capi-$TRIPLE-v$VER.tar.gz
sudo tar -xzf reflow-rt-capi-$TRIPLE-v$VER.tar.gz -C /usr/local
```

CMake:

```cmake
add_subdirectory(third_party/reflow/sdk/cpp)
target_link_libraries(myapp PRIVATE reflow::cpp)
```

If `find_library(reflow_rt_capi)` doesn't pick up the runtime automatically:

```cmake
set(REFLOW_RT_CAPI_LIB "/usr/local/lib/libreflow_rt_capi.dylib")
add_subdirectory(third_party/reflow/sdk/cpp)
```

## Hello world

```cpp
#include <reflow/reflow.hpp>
#include <atomic>
#include <chrono>
#include <thread>

int main() {
    reflow::Network net;

    auto doubler = reflow::Actor::from_callback(
        "doubler", {"in"}, {"out"},
        [](reflow::Context& ctx) {
            auto in = ctx.input_json("in");
            if (!in) return;
            auto pos = in->find("\"data\":");
            if (pos == std::string::npos) return;
            int64_t n = std::stoll(in->substr(pos + 7));
            ctx.emit("out", reflow::Message::integer(n * 2));
        });

    std::atomic<int64_t> got{0};
    auto collector = reflow::Actor::from_callback(
        "collector", {"in"}, {},
        [&](reflow::Context& ctx) {
            if (auto m = ctx.take_input("in")) {
                auto j = m->as_json();
                auto pos = j.find("\"data\":");
                if (pos != std::string::npos) got = std::stoll(j.substr(pos + 7));
            }
        });

    net.register_actor("tpl_doubler", std::move(doubler));
    net.register_actor("tpl_collector", std::move(collector));
    net.add_node("a", "tpl_doubler");
    net.add_node("b", "tpl_collector");
    net.add_connection("a", "out", "b", "in");
    net.add_initial("a", "in", R"({"type":"Integer","data":21})");
    net.start();

    std::this_thread::sleep_for(std::chrono::milliseconds(500));
    std::cout << "got: " << got.load() << "\n";  // → 42
    net.shutdown();
}
```

## Authoring graphs

```cpp
reflow::Graph g("demo");
g.add_node("a", "tpl_x");
g.add_node("b", "tpl_y");
g.add_connection("a", "out", "b", "in");
g.add_group("pipe", R"(["a","b"])", R"({"caption":"pipeline"})");
g.rename_node("a", "alpha");

if (auto node = g.get_node_json("alpha")) std::cout << *node << "\n";
std::cout << g.groups_json() << "\n";
std::cout << g.connections_json() << "\n";
```

`std::optional<std::string>` for nullable returns (`get_node_json`, `get_connection_json`); plain `std::string` everywhere else. Pick your own JSON parser — nlohmann/json, simdjson, RapidJSON, etc.

## Error handling

Every C ABI call is checked. Non-OK status throws `reflow::Error`, which carries the original `rfl_status` and the runtime's last error string:

```cpp
try {
    net.add_initial("missing-actor", "in", R"({"type":"Flow"})");
} catch (const reflow::Error& e) {
    std::cerr << e.what() << " status=" << e.status() << "\n";
}
```

## Packs

```cpp
auto templates = reflow::pack::load("./reflow.pack.ml-0.2.0.rflpack");
auto manifest  = reflow::pack::inspect_json("./reflow.pack.ml-0.2.0.rflpack");
auto loaded    = reflow::pack::list_json();
```

See [Packs](../packs/README.md) for the full pack workflow.

## Subgraphs

```cpp
reflow::SubgraphBuilder sub(graph_export_json);
sub.register_actor("my_custom", std::move(custom_actor));
sub.fill_from_catalog();
auto sg = sub.build();
net.register_actor("tpl_sub", std::move(sg));
```

## ABI lockstep

The C++ wrapper has no version of its own — it tracks the `libreflow_rt_capi` it links against. Pull the header from the same tag as the runtime tarball you install. `reflow::pack::abi_version()` returns the runtime's pack-ABI hash so you can sanity-check at startup.

## See also

- [Full C++ SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/cpp/README.md) — full CMake config + linkage notes.
- [`sdk/cpp/examples/quickstart.cpp`](https://github.com/offbit-ai/reflow/blob/main/sdk/cpp/examples/quickstart.cpp) — the worked end-to-end example.
- [Authoring Actors](../api/actors/creating-actors.md).
- [Packs](../packs/README.md).
