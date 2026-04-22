// Two-node pipeline: a Doubler actor emits 2×N into a Log actor.
// Run with:  node examples/double_and_log.mjs
// (requires `npm run build:debug` first so the native addon is built)
import {
  ReflowNetwork,
  ReflowActor,
  ReflowMessage,
} from "../index.js";

const double = ReflowActor.fromCallback(
  { component: "double", inports: ["in"], outports: ["out"] },
  (ctx) => {
    const inMsg = ctx.inputs?.in;
    const n = inMsg?.data ?? 0;
    ctx.done({ out: ReflowMessage.integer(n * 2).asJson() });
  }
);

const log = ReflowActor.fromCallback(
  { component: "log", inports: ["in"], outports: [] },
  (ctx) => {
    console.log("got:", ctx.inputs?.in);
    ctx.done();
  }
);

const net = new ReflowNetwork();
net.registerActor("tpl_double", double);
net.registerActor("tpl_log", log);

net.addNode("a", "tpl_double");
net.addNode("b", "tpl_log");
net.addConnection("a", "out", "b", "in");
net.addInitial("a", "in", { type: "Integer", data: 21 });

const events = net.events();
(async () => {
  for (let i = 0; i < 5; i++) {
    const evt = await events.recv();
    if (!evt) break;
    console.log("event:", evt);
  }
})();

net.start();
await new Promise((r) => setTimeout(r, 500));
net.shutdown();
