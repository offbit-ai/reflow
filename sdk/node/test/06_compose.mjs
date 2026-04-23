import { test } from "node:test";
import assert from "node:assert/strict";
import { composeGraphs } from "../reflow.mjs";

test("composeGraphs merges two GraphExports", () => {
  const composition = {
    graphs: [
      {
        caseSensitive: false,
        processes: { a: { id: "a", component: "tpl_passthrough", metadata: null } },
        connections: [],
        inports: {}, outports: {},
        properties: { name: "left" },
        groups: [],
      },
      {
        caseSensitive: false,
        processes: { b: { id: "b", component: "tpl_passthrough", metadata: null } },
        connections: [],
        inports: {}, outports: {},
        properties: { name: "right" },
        groups: [],
      },
    ],
    connections: [],
    shared_resources: [],
    properties: { name: "composed" },
    case_sensitive: false,
  };

  const out = composeGraphs(composition);
  const processes = Object.keys(out.processes);
  assert.ok(processes.some(k => k.endsWith("/a") || k === "a"),
    `missing 'a' in ${JSON.stringify(processes)}`);
  assert.ok(processes.some(k => k.endsWith("/b") || k === "b"),
    `missing 'b' in ${JSON.stringify(processes)}`);
});

test("composeGraphs wires cross-graph connections", () => {
  const composition = {
    graphs: [
      { caseSensitive: false,
        processes: { src: { id: "src", component: "tpl_passthrough", metadata: null } },
        connections: [], inports: {}, outports: {},
        properties: { name: "gsrc" }, groups: [] },
      { caseSensitive: false,
        processes: { sink: { id: "sink", component: "tpl_passthrough", metadata: null } },
        connections: [], inports: {}, outports: {},
        properties: { name: "gsink" }, groups: [] },
    ],
    connections: [
      { from: { process: "gsrc/src", port: "out" },
        to:   { process: "gsink/sink", port: "in" } },
    ],
    shared_resources: [],
    properties: { name: "pipeline" },
    case_sensitive: false,
  };

  const out = composeGraphs(composition);
  assert.ok(Array.isArray(out.connections));
  assert.ok(out.connections.length > 0, "expected at least one composed connection");
});
