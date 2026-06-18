import { test } from "node:test";
import assert from "node:assert/strict";
import { Actor, Message, Network } from "../reflow.mjs";

// First-class tracing in the Node SDK: enable via config, consume locally via
// Network.traces() (no collector required), and observe correlated events with
// content checksums.

class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];
  run(ctx) {
    const n = (ctx.inputs?.in && (ctx.inputs.in.data ?? 0)) || 0;
    ctx.done({ out: Message.integer(Number(n) * 2) });
  }
}

class Collect extends Actor {
  static component = "collect";
  static inports = ["in"];
  static outports = [];
  constructor(sink) {
    super();
    this.sink = sink;
  }
  run(ctx) {
    if (ctx.inputs?.in) this.sink.push(ctx.inputs.in);
    ctx.done();
  }
}

function eventTypeNames(evt) {
  const et = evt.event_type;
  if (typeof et === "string") return [et];
  if (et && typeof et === "object") return Object.keys(et);
  return [];
}

test("Network.traces() yields correlated trace events with checksums", async () => {
  const sink = [];
  // Tracing enabled. The collector URL need not be reachable — events still
  // flow to the local tap.
  const net = new Network({
    tracing: { server_url: "ws://127.0.0.1:8080", enabled: true },
  });
  net.registerActor("tpl_doubler", new Doubler());
  net.registerActor("tpl_collect", new Collect(sink));
  net.addNode("d", "tpl_doubler");
  net.addNode("c", "tpl_collect");
  net.addConnection("d", "out", "c", "in");
  net.addInitial("d", "in", { type: "Integer", data: 21 });

  const traces = net.traces();
  net.start();

  const seen = new Set();
  let sawChecksum = false;
  const deadline = Date.now() + 3000;
  while (Date.now() < deadline) {
    // recv() resolves null on close; race a timeout so we never hang.
    const evt = await Promise.race([
      traces.recv(),
      new Promise((r) => setTimeout(() => r(undefined), 250)),
    ]);
    if (!evt) continue;
    for (const name of eventTypeNames(evt)) seen.add(name);
    const msg = evt.data && evt.data.message;
    if (msg && typeof msg.checksum === "string" && msg.checksum.startsWith("sha256:")) {
      sawChecksum = true;
    }
    if (seen.has("ActorCreated") && sawChecksum) break;
  }

  net.shutdown();

  assert.ok(seen.has("ActorCreated"), `seen event types: ${[...seen].join(", ")}`);
  assert.ok(sawChecksum, "expected a data-flow snapshot carrying a sha256 checksum");
});
