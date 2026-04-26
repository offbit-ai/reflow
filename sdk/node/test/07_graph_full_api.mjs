import { test } from "node:test";
import assert from "node:assert/strict";
import { Graph } from "../reflow.mjs";

test("Graph: rename node propagates to connections", () => {
  const g = new Graph("rename-demo", false);
  g.addNode("a", "tpl_x");
  g.addNode("b", "tpl_y");
  g.addConnection("a", "out", "b", "in");

  g.renameNode("a", "alpha");

  const json = g.toJson();
  assert.ok(json.processes.alpha);
  assert.equal(json.processes.a, undefined);
  // The connection should now reference the new id.
  const conn = g.getConnection("alpha", "out", "b", "in");
  assert.ok(conn, "connection should resolve under the new node id");
});

test("Graph: groups CRUD round-trip", () => {
  const g = new Graph("groups-demo", false);
  g.addNode("a", "tpl_x");
  g.addNode("b", "tpl_y");
  g.addNode("c", "tpl_z");

  g.addGroup("g1", ["a", "b"], { tag: "left" });
  g.addToGroup("g1", "c");
  g.removeFromGroup("g1", "a");
  g.setGroupMetadata("g1", { tag: "right" });

  const groups = g.groups();
  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, "g1");
  assert.deepEqual(new Set(groups[0].nodes), new Set(["b", "c"]));
  assert.equal(groups[0].metadata?.tag, "right");

  g.removeGroup("g1");
  assert.equal(g.groups().length, 0);
});

test("Graph: ports + port-rename + port-metadata", () => {
  const g = new Graph("ports-demo", false);
  g.addNode("a", "tpl_x");
  // PortType is adjacently tagged — pass null for default (Any) or
  // an object like { type: "flow" }.
  g.addInport("input", "a", "in", { type: "flow" }, { caption: "input" });
  g.addOutport("output", "a", "out", { type: "flow" }, { caption: "output" });

  g.renameInport("input", "left");
  g.renameOutport("output", "right");
  g.setInportMetadata("left", { caption: "L" });
  g.setOutportMetadata("right", { caption: "R" });

  const inports = g.inports();
  const outports = g.outports();
  assert.ok(inports.left);
  assert.ok(outports.right);
  assert.equal(inports.left.metadata?.caption, "L");
  assert.equal(outports.right.metadata?.caption, "R");

  g.removeInport("left");
  g.removeOutport("right");
  assert.equal(Object.keys(g.inports()).length, 0);
  assert.equal(Object.keys(g.outports()).length, 0);
});

test("Graph: connection + initial removal + connection metadata", () => {
  const g = new Graph("conn-demo", false);
  g.addNode("a", "tpl_x");
  g.addNode("b", "tpl_y");
  g.addConnection("a", "out", "b", "in");
  g.setConnectionMetadata("a", "out", "b", "in", { weight: 1 });
  g.addInitial("a", "in", { type: "Integer", data: 42 });

  // `connections()` is regular connections only; initials live in
  // `initializers()`. (The combined view is what `toJson()` flattens
  // into its `.connections` array.)
  assert.equal(g.connections().length, 1);
  assert.equal(g.initializers().length, 1);
  const c = g.getConnection("a", "out", "b", "in");
  assert.equal(c.metadata?.weight, 1);

  g.removeConnection("a", "out", "b", "in");
  g.removeInitial("a", "in");
  assert.equal(g.connections().length, 0);
  assert.equal(g.initializers().length, 0);
});

test("Graph: graph-level initials via exposed inports", () => {
  const g = new Graph("graph-init", false);
  g.addNode("a", "tpl_x");
  g.addInport("entry", "a", "in", { type: "flow" });

  g.addGraphInitial("entry", { type: "Integer", data: 7 });
  assert.equal(g.initializers().length, 1);

  g.removeGraphInitial("entry");
  assert.equal(g.initializers().length, 0);
});

test("Graph: setProperties round-trip", () => {
  const g = new Graph("props", false);
  g.setProperties({ author: "darmie", domain: "reflow" });
  assert.equal(g.properties().author, "darmie");
  assert.equal(g.properties().domain, "reflow");
});

test("Graph: import replaces graph state with another export", () => {
  // `import` is destructive — it clears existing nodes / connections /
  // properties and reloads from the supplied GraphExport.
  const seed = new Graph("seed", false);
  seed.addNode("x", "tpl_x");
  seed.addNode("y", "tpl_y");
  seed.addConnection("x", "out", "y", "in");

  const target = new Graph("target", false);
  target.addNode("doomed", "tpl_z");
  target.import(seed.toJson());

  assert.ok(target.getNode("x"));
  assert.ok(target.getNode("y"));
  assert.equal(target.getNode("doomed"), null);
  assert.equal(target.connections().length, 1);
});

test("Graph: queries (nodes/connections/getNode)", () => {
  const g = new Graph("queries", false);
  g.addNode("a", "tpl_x");
  g.addNode("b", "tpl_y");
  g.addConnection("a", "out", "b", "in");

  const nodes = g.nodes();
  assert.equal(nodes.length, 2);
  const a = g.getNode("a");
  assert.equal(a.component, "tpl_x");
  assert.equal(g.getNode("nope"), null);
});
