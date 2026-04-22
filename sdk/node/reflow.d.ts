// High-level TypeScript types for @offbit-ai/reflow.
// Layers an Actor base class on top of the auto-generated bindings.

import {
  ReflowActor,
  ReflowNetwork,
  ReflowGraph,
  ReflowMessage,
  ReflowStream,
  StreamReader,
  SubgraphBuilder,
  EventStream,
  ActorCallContext,
  ActorOptions,
  StreamOptions,
  StreamBeginOptions,
  StreamFrameValue,
  templateActor,
  templateList,
} from "./index";

/**
 * Base class for JS-authored actors. Extend and override `run(ctx)`.
 *
 * ```
 * class Doubler extends Actor {
 *   static component = "doubler";
 *   static inports = ["in"];
 *   static outports = ["out"];
 *   run(ctx) { ctx.done({ out: Message.integer(ctx.inputs.in.data * 2) }); }
 * }
 * net.registerActor("tpl_doubler", new Doubler());
 * ```
 */
export declare class Actor {
  static component: string;
  static inports: readonly string[];
  static outports: readonly string[];
  static awaitAllInports: boolean;

  run(ctx: ActorCallContext): void | Promise<void>;
}

// Friendly aliases that match the @offbit-ai/reflow package name.
export declare const Network: typeof ReflowNetwork;
export declare const Graph: typeof ReflowGraph;
export declare const Message: typeof ReflowMessage;
export declare const Stream: typeof ReflowStream;

// Patched registerActor signatures accept either a raw ReflowActor or an
// Actor class instance. The declaration merges with the base class.
declare module "./index" {
  interface ReflowNetwork {
    registerActor(templateId: string, actor: ReflowActor | Actor): void;
  }
  interface SubgraphBuilder {
    registerActor(componentName: string, actor: ReflowActor | Actor): void;
  }
}

export {
  ReflowActor,
  ReflowNetwork,
  ReflowGraph,
  ReflowMessage,
  ReflowStream,
  StreamReader,
  SubgraphBuilder,
  EventStream,
  ActorCallContext,
  ActorOptions,
  StreamOptions,
  StreamBeginOptions,
  StreamFrameValue,
  templateActor,
  templateList,
};
