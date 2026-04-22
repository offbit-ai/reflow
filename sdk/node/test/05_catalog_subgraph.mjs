import { test } from "node:test";
import assert from "node:assert/strict";
import {
  Actor,
  Network,
  SubgraphBuilder,
  templateActor,
  templateList,
} from "../reflow.mjs";

test("templateList is non-empty and templateActor resolves a known id", () => {
  const ids = templateList();
  assert.ok(ids.length > 100, `unexpectedly small catalog: ${ids.length}`);
  assert.ok(ids.includes("tpl_passthrough"));

  const a = templateActor("tpl_passthrough");
  assert.ok(a, "templateActor('tpl_passthrough') should produce a handle");
});

test("templateActor throws on unknown id", () => {
  assert.throws(() => templateActor("tpl_this_does_not_exist"));
});

test("SubgraphBuilder composes a graph export with registered JS actors", () => {
  class Tap extends Actor {
    static component = "my_tap";
    static inports = ["in"];
    static outports = ["out"];
    run(ctx) { ctx.done({ out: ctx.inputs?.in }); }
  }

  const exportJson = {
    caseSensitive: false,
    processes: {
      t: { id: "t", component: "my_tap", metadata: null },
    },
    connections: [],
    inports: {},
    outports: {},
    properties: { name: "sg" },
    groups: [],
  };

  const builder = new SubgraphBuilder(exportJson);
  builder.registerActor("my_tap", new Tap());
  const sg = builder.build();

  const net = new Network();
  net.registerActor("tpl_sub", sg);
  net.addNode("s", "tpl_sub");
  net.shutdown();
});
