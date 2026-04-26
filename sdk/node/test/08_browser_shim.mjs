// Smoke-test the browser shim (`reflow.browser.mjs`) against the
// Node SDK API surface. Runs in Node by feeding the .wasm to the
// wasm-bindgen `init()` directly — `wasm-pack --target web`
// otherwise tries to `fetch()` the .wasm relative to the module URL,
// which only works in browsers.
//
// The point of this test is structural: every method we promise on
// the Node SDK exists on the browser shim with the same name and
// the same argument order. We don't assert on the runtime
// behavior of GraphNetwork here — that needs a real event loop and
// is exercised by the napi tests (03_actor_network).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Skip the whole file if the wasm bundle hasn't been built yet —
// CI builds it before running tests, but a fresh `npm test` on a
// new clone will skip cleanly rather than fail.
const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(__dirname, "..", "wasm", "reflow_rt_wasm_bg.wasm");
let bundle;
try {
  bundle = readFileSync(wasmPath);
} catch {
  test.skip("browser shim — wasm bundle not built (run wasm-pack first)", () => {});
  // No `process.exit` — the runner picks up `test.skip` and continues.
}

if (bundle) {
  const shim = await import("../reflow.browser.mjs");
  await shim.ready(bundle);

  test("Message helpers produce the canonical JSON shape", () => {
    assert.deepEqual(shim.Message.flow(), { type: "Flow" });
    assert.deepEqual(shim.Message.integer(42), { type: "Integer", data: 42 });
    assert.deepEqual(shim.Message.boolean(true), { type: "Boolean", data: true });
    assert.deepEqual(shim.Message.string("hi"), { type: "String", data: "hi" });
  });

  test("Graph: construction, addNode, getNode", () => {
    const g = new shim.Graph("demo", false);
    g.addNode("a", "tpl_x", null);
    g.addNode("b", "tpl_y", { role: "sink" });
    const node = g.getNode("a");
    assert.equal(node?.component, "tpl_x");
  });

  test("Graph: addInitial uses Node-SDK arg order", () => {
    const g = new shim.Graph("demo", false);
    g.addNode("a", "tpl_x", null);
    // Node order: (node, port, data, metadata?)
    g.addInitial("a", "in", shim.Message.flow(), null);
    const json = g.toJson();
    const inits = json.connections.filter((c) => c.data != null);
    assert.equal(inits.length, 1);
    // Initial should land on node "a", port "in" — proves the
    // shim translated `(node, port, data)` to wasm's
    // `(data, node, port)`.
    assert.equal(inits[0].to.nodeId, "a");
    assert.equal(inits[0].to.portId, "in");
  });

  test("Graph: query getters use Node-SDK names", () => {
    const g = new shim.Graph("demo", false);
    g.addNode("a", "tpl_x", null);
    g.addNode("b", "tpl_y", null);
    g.addConnection("a", "out", "b", "in", null);
    g.addGroup("group1", ["a", "b"], null);

    // Node uses bare-noun getters; the shim aliases them to the
    // wasm's getNodes()/getConnections()/getGroups().
    assert.ok(g.nodes(), "nodes() should be defined");
    assert.ok(g.connections(), "connections() should be defined");
    assert.ok(g.groups(), "groups() should be defined");
    assert.ok(g.initializers() != null, "initializers() should be defined");
    assert.ok(g.properties() != null, "properties() should be defined");
  });

  test("Graph: toJson and toJSON return the same payload", () => {
    const g = new shim.Graph("demo", false);
    g.addNode("a", "tpl_x", null);
    assert.deepEqual(g.toJson(), g.toJSON());
  });

  test("Graph.fromJson and Graph.load both work", () => {
    const src = {
      caseSensitive: false,
      processes: { n: { id: "n", component: "tpl_passthrough", metadata: null } },
      connections: [],
      inports: {},
      outports: {},
      properties: { name: "demo" },
      groups: [],
    };
    const g1 = shim.Graph.fromJson(src);
    const g2 = shim.Graph.load(src);
    assert.equal(g1.toJson().processes.n.component, "tpl_passthrough");
    assert.equal(g2.toJson().processes.n.component, "tpl_passthrough");
  });

  test("Graph: full Tier-1 mutator surface is reachable", () => {
    const g = new shim.Graph("demo", false);
    g.addNode("a", "tpl_x");
    g.addNode("b", "tpl_y");
    // exercise a sweep of the Tier-1 mutators to make sure the
    // shim forwards each to the underlying wasm class
    const flowPort = { type: "flow" };
    g.addConnection("a", "out", "b", "in", null);
    g.addInport("entry", "a", "in", flowPort, null);
    g.addOutport("exit", "b", "out", flowPort, null);
    g.addGroup("g1", ["a", "b"], null);
    g.addToGroup("g1", "a");
    g.removeFromGroup("g1", "a");
    g.setNodeMetadata("a", { x: 10 });
    g.setConnectionMetadata("a", "out", "b", "in", { weight: 1 });
    g.setInportMetadata("entry", { color: "blue" });
    g.setOutportMetadata("exit", { color: "red" });
    g.setGroupMetadata("g1", { label: "core" });
    g.setProperties({ author: "test" });
    g.renameNode("a", "alpha");
    g.renameInport("entry", "input");
    g.renameOutport("exit", "output");
    // graph-level initials use the Node order (inport, data, [index,] metadata?)
    g.addGraphInitial("input", shim.Message.integer(1), null);
    g.addGraphInitialIndex("input", shim.Message.integer(2), 0, null);
    g.removeGraphInitial("input");
    // round-trip through toJson to ensure the underlying graph isn't
    // corrupted by any of the above
    const json = g.toJson();
    assert.ok(json.processes.alpha);
  });

  test("Network: constructor + fromGraph + start preconditions", () => {
    const n1 = new shim.Network();
    assert.ok(n1.graph instanceof shim.Graph);
    const g = new shim.Graph("from_graph_path");
    g.addNode("a", "tpl_x");
    const n2 = shim.Network.fromGraph(g);
    assert.strictEqual(n2.graph, g);
  });

  test("Network: imperative API delegates to graph", () => {
    const n = new shim.Network();
    n.addNode("a", "tpl_x", null);
    n.addNode("b", "tpl_y", null);
    n.addConnection("a", "out", "b", "in");
    n.addInitial("a", "in", shim.Message.flow());
    const json = n.graph.toJson();
    assert.ok(json.processes.a);
    assert.ok(json.processes.b);
    assert.equal(json.connections.length, 2); // 1 regular + 1 initial
  });

  test("Network: events() throws before start (matches Node lifecycle)", () => {
    const n = new shim.Network();
    assert.throws(() => n.events(), /start\(\) first/);
  });

  test("Actor: subclass + _build produces the wasm-shaped object", () => {
    class Doubler extends shim.Actor {
      static component = "doubler";
      static inports = ["in"];
      static outports = ["out"];
      run(ctx) {
        ctx.send({ out: shim.Message.integer(2 * ctx.input.in.data) });
        ctx.done();
      }
    }
    const a = new Doubler();
    const built = a._build();
    assert.deepEqual(built.inports, ["in"]);
    assert.deepEqual(built.outports, ["out"]);
    assert.equal(typeof built.run, "function");
  });

  test("default export shape matches named exports", () => {
    assert.equal(shim.default.Graph, shim.Graph);
    assert.equal(shim.default.Network, shim.Network);
    assert.equal(shim.default.Actor, shim.Actor);
    assert.equal(shim.default.Message, shim.Message);
    assert.equal(shim.default.bindInputEvents, shim.bindInputEvents);
    assert.equal(typeof shim.default.version, "function");
    assert.equal(typeof shim.default.ready, "function");
    assert.equal(typeof shim.default.loadPack, "function");
    assert.equal(typeof shim.default.packAbiVersion, "function");
    assert.equal(typeof shim.default.initGpuContext, "function");
  });

  test("packAbiVersion returns a non-zero u32", () => {
    const v = shim.packAbiVersion();
    assert.equal(typeof v, "number");
    assert.ok(v > 0, "ABI version should be populated by build.rs");
  });

  test("loadPack rejects unfetchable URLs", async () => {
    // file:// URLs aren't fetchable from Node fetch in a portable
    // way, and a non-existent http URL would race against network
    // conditions. Use a clearly-invalid scheme so fetch fails fast.
    await assert.rejects(
      () => shim.loadPack("not-a-real-scheme://nope"),
      /loadPack|fetch|TypeError/i,
    );
  });

  test("loadPack rejects non-zip payloads with a clear error", async () => {
    // Stand up a minimal HTTP server that serves invalid bytes —
    // exercises the fetch → extract path without depending on a
    // real .rflpack being available.
    const { createServer } = await import("node:http");
    const server = createServer((_req, res) => {
      res.writeHead(200, { "content-type": "application/octet-stream" });
      res.end(Buffer.from("not a zip archive"));
    });
    await new Promise((r) => server.listen(0, "127.0.0.1", r));
    const { port } = server.address();
    try {
      await assert.rejects(
        () => shim.loadPack(`http://127.0.0.1:${port}/x.rflpack`),
        /zip|parse|.rflpack/i,
      );
    } finally {
      server.close();
    }
  });
}
