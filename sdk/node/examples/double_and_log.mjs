// Two-node pipeline: a Doubler actor emits 2×N into a Log actor.
// Run with:  node examples/double_and_log.mjs
// (requires `npm run build` or `npm run build:debug` first)
import { Actor, Network, Message } from "../reflow.mjs";

class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];

  run(ctx) {
    const n = Number(ctx.inputs?.in?.data ?? 0);
    ctx.done({ out: Message.integer(n * 2) });
  }
}

class Log extends Actor {
  static component = "log";
  static inports = ["in"];
  static outports = [];

  constructor(label) {
    super();
    this.label = label ?? "got";
  }

  run(ctx) {
    console.log(`${this.label}:`, ctx.inputs?.in);
    ctx.done();
  }
}

const net = new Network();
net.registerActor("tpl_doubler", new Doubler());
net.registerActor("tpl_log", new Log("doubled"));

net.addNode("a", "tpl_doubler");
net.addNode("b", "tpl_log");
net.addConnection("a", "out", "b", "in");
net.addInitial("a", "in", { type: "Integer", data: 21 });

net.start();
await new Promise((r) => setTimeout(r, 200));
net.shutdown();
