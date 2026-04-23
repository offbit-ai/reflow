// High-level JS API for @offbit-ai/reflow.
// Wraps the low-level napi-rs bindings in `./index.js`, adds a class-based
// Actor pattern, and exposes idiomatic names (Network, Graph, Message, ...).

'use strict';

const native = require('./index.js');

/**
 * Base class for JS-authored actors.
 *
 * Subclass and override `run(ctx)`. Declare ports and await semantics
 * via static fields:
 *
 *   class Doubler extends Actor {
 *     static component = "doubler";
 *     static inports = ["in"];
 *     static outports = ["out"];
 *     run(ctx) { ctx.done({ out: Message.integer(2 * ctx.inputs.in.data) }); }
 *   }
 *
 *   net.registerActor("tpl_doubler", new Doubler());
 */
class Actor {
  static component = "anonymous";
  static inports = [];
  static outports = [];
  static awaitAllInports = false;

  /**
   * Called by the runtime on every tick the actor has inputs.
   * Must resolve the tick by calling exactly one of `ctx.done(outputs?)`
   * or `ctx.fail(message)`. May be async — throw / reject to auto-fail.
   *
   * @param {import('./index.d.ts').ActorCallContext} ctx
   */
  run(_ctx) {
    throw new Error(`${this.constructor.name}.run() is not implemented`);
  }

  /** @internal — build the Rust-side actor handle. */
  _build() {
    const cls = this.constructor;
    const options = {
      component: cls.component,
      inports: Array.from(cls.inports ?? []),
      outports: Array.from(cls.outports ?? []),
      awaitAllInports: !!cls.awaitAllInports,
    };
    const self = this;
    return native.ReflowActor.fromCallback(options, (ctx) => {
      let finished = false;
      const done = ctx.done.bind(ctx);
      const fail = ctx.fail.bind(ctx);
      ctx.done = (outputs) => {
        if (finished) return;
        finished = true;
        const cleaned = normalizeOutputs(outputs);
        return done(cleaned);
      };
      ctx.fail = (msg) => {
        if (finished) return;
        finished = true;
        return fail(String(msg ?? "actor failed"));
      };
      try {
        const result = self.run(ctx);
        if (result && typeof result.then === "function") {
          result.catch((err) => ctx.fail(err?.stack || err?.message || String(err)));
        }
      } catch (err) {
        ctx.fail(err?.stack || err?.message || String(err));
      }
    });
  }
}

function normalizeOutputs(outputs) {
  if (outputs == null) return {};
  if (typeof outputs !== "object" || Array.isArray(outputs)) {
    throw new TypeError("ctx.done(outputs) expects an object keyed by port");
  }
  const out = {};
  for (const [port, value] of Object.entries(outputs)) {
    if (value == null) continue;
    // Accept either a Message handle (call .asJson()) or a plain
    // JSON-shaped Message ({type, data?}) the Rust side can deserialize.
    if (typeof value.asJson === "function") {
      out[port] = value.asJson();
    } else {
      out[port] = value;
    }
  }
  return out;
}

// Patch registerActor on Network / SubgraphBuilder to accept Actor instances.
function wrapRegister(cls) {
  const orig = cls.prototype.registerActor;
  cls.prototype.registerActor = function (templateId, actor) {
    if (actor == null) {
      throw new Error("registerActor(templateId, actor) requires two arguments");
    }
    const wrapped = actor instanceof Actor ? actor._build() : actor;
    return orig.call(this, templateId, wrapped);
  };
}
wrapRegister(native.ReflowNetwork);
wrapRegister(native.SubgraphBuilder);

module.exports = {
  // High-level JS API
  Actor,

  // Friendlier aliases for the Rust-defined classes
  Network: native.ReflowNetwork,
  Graph: native.ReflowGraph,
  Message: native.ReflowMessage,
  Stream: native.ReflowStream,
  StreamReader: native.StreamReader,
  SubgraphBuilder: native.SubgraphBuilder,
  EventStream: native.EventStream,
  ActorCallContext: native.ActorCallContext,

  // Template catalog
  templateActor: native.templateActor,
  templateList: native.templateList,

  // Multi-graph composition
  composeGraphs: native.composeGraphs,

  // Raw bindings (escape hatch)
  ReflowActor: native.ReflowActor,
  ReflowNetwork: native.ReflowNetwork,
  ReflowGraph: native.ReflowGraph,
  ReflowMessage: native.ReflowMessage,
  ReflowStream: native.ReflowStream,
};
