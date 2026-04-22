import { test } from "node:test";
import assert from "node:assert/strict";
import { Graph } from "../reflow.mjs";

test("Graph construction and toJson", () => {
  const g = new Graph("demo", false);
  g.addNode("a", "tpl_x", null);
  g.addNode("b", "tpl_y", { role: "sink" });
  g.addConnection("a", "out", "b", "in", null);
  g.addInitial("a", "in", { type: "Flow" }, null);

  const json = g.toJson();
  assert.equal(typeof json, "object");
  assert.ok(json.processes?.a);
  assert.equal(json.processes.a.component, "tpl_x");
  assert.equal(json.processes.b.metadata?.role, "sink");
  // addConnection + addInitial both materialize as entries in `connections`;
  // initials carry `data`, regular connections have null data.
  const regular = json.connections.filter((c) => c.data == null);
  const inits = json.connections.filter((c) => c.data != null);
  assert.equal(regular.length, 1);
  assert.equal(regular[0].from.nodeId, "a");
  assert.equal(inits.length, 1);
});

test("Graph.fromJson round-trips", () => {
  const src = {
    caseSensitive: false,
    processes: { n: { id: "n", component: "tpl_passthrough", metadata: null } },
    connections: [],
    inports: {},
    outports: {},
    properties: { name: "demo" },
    groups: [],
  };
  const g = Graph.fromJson(src);
  const out = g.toJson();
  assert.equal(out.processes.n.component, "tpl_passthrough");
});

test("Graph.removeNode clears the node", () => {
  const g = new Graph("demo");
  g.addNode("a", "tpl_x");
  g.removeNode("a");
  assert.equal(Object.keys(g.toJson().processes).length, 0);
});
