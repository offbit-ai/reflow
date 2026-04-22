import { test } from "node:test";
import assert from "node:assert/strict";
import { Actor, Message, Network } from "../reflow.mjs";

// Doubler actor as a class — demonstrates the intended authoring pattern.
class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];

  run(ctx) {
    const inMsg = ctx.inputs?.in;
    const n = (inMsg && (inMsg.data ?? 0)) || 0;
    ctx.done({ out: Message.integer(Number(n) * 2) });
  }
}

// Collector actor — closes over a shared array so the test can observe
// what reached the end of the pipeline.
class Collect extends Actor {
  static component = "collect";
  static inports = ["in"];
  static outports = [];

  constructor(sink) {
    super();
    this.sink = sink;
  }

  run(ctx) {
    const msg = ctx.inputs?.in;
    if (msg) this.sink.push(msg);
    ctx.done();
  }
}

test("Network runs a 2-node pipeline end-to-end", async (t) => {
  const sink = [];
  const net = new Network();
  net.registerActor("tpl_doubler", new Doubler());
  net.registerActor("tpl_collect", new Collect(sink));

  net.addNode("d", "tpl_doubler");
  net.addNode("c", "tpl_collect");
  net.addConnection("d", "out", "c", "in");
  net.addInitial("d", "in", { type: "Integer", data: 21 });

  const events = net.events();
  net.start();

  // Wait up to 1s for the doubled value to reach the collector.
  const deadline = Date.now() + 1000;
  while (Date.now() < deadline) {
    if (sink.length > 0) break;
    await new Promise((r) => setTimeout(r, 20));
  }

  net.shutdown();

  assert.equal(sink.length, 1, `sink: ${JSON.stringify(sink)}`);
  assert.equal(sink[0].type, "Integer");
  assert.equal(sink[0].data, 42);
});

test("event stream yields NetworkStarted + ActorStarted", async () => {
  class Nop extends Actor {
    static component = "nop";
    static inports = ["in"];
    static outports = [];
    run(ctx) { ctx.done(); }
  }

  const net = new Network();
  net.registerActor("tpl_nop", new Nop());
  net.addNode("n", "tpl_nop");

  const events = net.events();
  net.start();

  const seen = new Set();
  const deadline = Date.now() + 1500;
  while (Date.now() < deadline && (!seen.has("NetworkStarted") || !seen.has("ActorStarted"))) {
    const timeout = new Promise((r) => setTimeout(() => r("__t__"), 150));
    const evt = await Promise.race([events.recv(), timeout]);
    if (evt === "__t__") continue;
    if (!evt) break;
    if (evt._type) seen.add(evt._type);
  }

  net.shutdown();
  assert.ok(seen.has("NetworkStarted"), `missing NetworkStarted; saw ${[...seen]}`);
  assert.ok(seen.has("ActorStarted"), `missing ActorStarted; saw ${[...seen]}`);
});
