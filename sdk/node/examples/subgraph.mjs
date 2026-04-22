// Compose a subgraph out of a JS-authored actor, then register the whole
// subgraph as a single template on the parent network.
import { Actor, Network, SubgraphBuilder } from "../reflow.mjs";

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
  properties: { name: "tap-subgraph" },
  groups: [],
};

const builder = new SubgraphBuilder(exportJson);
builder.registerActor("my_tap", new Tap());
const sg = builder.build();
console.log("built subgraph actor");

const net = new Network();
net.registerActor("tpl_sub", sg);
net.addNode("s", "tpl_sub");
console.log("registered + added to network");
net.shutdown();
