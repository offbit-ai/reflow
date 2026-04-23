// ESM entry point for @offbit-ai/reflow.
// Re-exports from the CJS wrapper so both import styles work.

import mod from "./reflow.js";

export const {
  Actor,
  Network,
  Graph,
  Message,
  Stream,
  StreamReader,
  SubgraphBuilder,
  EventStream,
  ActorCallContext,
  templateActor,
  templateList,
  composeGraphs,
  ReflowActor,
  ReflowNetwork,
  ReflowGraph,
  ReflowMessage,
  ReflowStream,
} = mod;

export default mod;
