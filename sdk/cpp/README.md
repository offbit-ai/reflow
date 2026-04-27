# Reflow — C++ SDK

Header-only C++17 wrapper over `libreflow_rt_capi`. Same RAII shape
the other SDKs use: every handle is owned by a `unique_ptr` with a
custom deleter, errors throw `reflow::Error`, and JSON-shaped returns
come back as `std::string` so you can plug in whatever JSON library
you prefer.

```cpp
#include <reflow/reflow.hpp>

reflow::Graph g("demo");
g.add_node("a", "tpl_x");
g.add_node("b", "tpl_y");
g.add_connection("a", "out", "b", "in");
g.add_initial("a", "in", R"({"type":"Integer","data":42})");
std::cout << g.to_json() << "\n";
```

## Requirements

- C++17 compiler (clang ≥ 9, gcc ≥ 9, MSVC 2019+)
- `libreflow_rt_capi.{so,dylib,dll}` for your platform

The runtime library is **not bundled** with this header. Two paths:

### (A) Pre-built tarball from GitHub Releases

The Go SDK's `publish-go` workflow ships per-triple tarballs to
[Releases](https://github.com/offbit-ai/reflow/releases?q=sdk%2Fgo) on
every `sdk/go/v*` tag. Each tarball contains exactly the files this
SDK needs — `lib/libreflow_rt_capi.{dylib,so,dll}` and
`include/reflow_rt.h`. Download, untar, and point CMake at it.

```sh
VER=0.2.3
TRIPLE=aarch64-apple-darwin   # or x86_64-apple-darwin, x86_64-unknown-linux-gnu, …
curl -LO https://github.com/offbit-ai/reflow/releases/download/sdk/go/v$VER/reflow-rt-capi-$TRIPLE-v$VER.tar.gz
tar -xzf reflow-rt-capi-$TRIPLE-v$VER.tar.gz -C /usr/local
```

### (B) Build from source (monorepo)

```sh
cargo build -p reflow_rt_capi --release
# → target/release/libreflow_rt_capi.{dylib,so,dll}
```

## Integrating with CMake

```cmake
add_subdirectory(third_party/reflow/sdk/cpp)
target_link_libraries(myapp PRIVATE reflow::cpp)
```

If `find_library(reflow_rt_capi)` doesn't pick up the runtime, point
it explicitly:

```cmake
set(REFLOW_RT_CAPI_LIB "/usr/local/lib/libreflow_rt_capi.dylib")
add_subdirectory(third_party/reflow/sdk/cpp)
```

Or do the install + `find_package` flow:

```sh
cmake -S sdk/cpp -B build && cmake --install build --prefix /usr/local
```

```cmake
find_package(reflow REQUIRED)
target_link_libraries(myapp PRIVATE reflow::cpp)
```

## API surface

The header mirrors `crates/reflow_rt_capi/include/reflow_rt.h` 1:1
with C++ ergonomics:

| C ABI                                    | C++                              |
|------------------------------------------|----------------------------------|
| `rfl_graph_*`                            | `reflow::Graph`                  |
| `rfl_network_*`                          | `reflow::Network`                |
| `rfl_actor_new` / `rfl_template_actor_new` | `reflow::Actor`                |
| `rfl_message_*`                          | `reflow::Message`                |
| `rfl_events_*`                           | `reflow::EventStream`            |
| `rfl_subgraph_builder_*`                 | `reflow::SubgraphBuilder`        |
| `rfl_template_list_json`                 | `reflow::template_list_json()`   |
| `rfl_compose_graphs`                     | `reflow::compose_graphs(json)`   |
| `rfl_pack_load` / `rfl_pack_*`           | `reflow::pack::load(...)` etc.   |

### Error handling

Every C call that returns `rfl_status` is checked; non-OK throws
`reflow::Error`, whose `what()` includes both the operation and the
runtime's last error message (the same string `rfl_last_error_message`
returns). The exception also carries the original `rfl_status` via
`error.status()` if you need to discriminate programmatically.

```cpp
try {
    g.remove_node("nope");
} catch (const reflow::Error& e) {
    std::cerr << e.what() << " (status=" << e.status() << ")\n";
}
```

### JSON returns

Methods like `Graph::nodes_json()` / `connections_json()` /
`get_node_json(id)` return `std::string` (or `std::optional<std::string>`
for query misses). The library doesn't bundle a JSON parser — pick
[nlohmann/json](https://github.com/nlohmann/json),
[simdjson](https://github.com/simdjson/simdjson), or whatever you
already use.

```cpp
#include <nlohmann/json.hpp>
auto nodes = nlohmann::json::parse(g.nodes_json());
for (auto& n : nodes) std::cout << n["id"].get<std::string>() << "\n";
```

### Authoring an actor in C++

```cpp
auto doubler = reflow::Actor::from_callback(
    "doubler", {"in"}, {"out"},
    [](rfl_actor_ctx* ctx) -> rfl_status {
        char* in = rfl_ctx_input_json(ctx, "in");
        if (!in) return rfl_status_Ok;            // no input this tick
        // … parse, compute, emit …
        rfl_string_free(in);
        return rfl_status_Ok;
    });

reflow::Network net;
net.register_actor("tpl_doubler", std::move(doubler));
```

## Building the example

```sh
cmake -S sdk/cpp -B build -DREFLOW_CPP_BUILD_EXAMPLES=ON \
      -DREFLOW_RT_CAPI_LIB=$PWD/target/release/libreflow_rt_capi.dylib
cmake --build build
build/reflow_cpp_quickstart
```

## Versioning

The C++ wrapper tracks the C ABI version of the runtime it was built
against. There's no separate package: pull the header alongside any
matching `libreflow_rt_capi` release. The header's
`reflow::pack::abi_version()` returns the same number as the runtime,
so you can sanity-check at startup if you load packs.

## License

MIT OR Apache-2.0, matching the rest of the repository.
