// TypeScript declarations for the browser entrypoint.
//
// The browser shim wraps the wasm-bindgen build and re-exposes the
// classes with the same names + argument shapes as the Node SDK.
// Where the Node SDK has rich types (`reflow.d.ts`), we re-use them
// here so call signatures match across targets.

import type {
  GraphExport,
  GraphConnection,
  GraphNode,
  PortType,
} from "./wasm/reflow_rt_wasm";

/**
 * Initialize the wasm module. Idempotent — subsequent calls return
 * the cached promise. Must be awaited before any Graph / Network /
 * Actor use.
 */
export function ready(
  moduleOrUrl?: RequestInfo | URL | BufferSource | WebAssembly.Module,
): Promise<unknown>;

/** Reflow library version string. */
export function version(): string;

/**
 * Bind DOM input events (keyboard, mouse, wheel, touch, resize) to a
 * running `Network`. Events are routed to actors whose component id
 * matches `tpl_keyboard_input`, `tpl_mouse_input`, etc. Returns a
 * cleanup callback that removes every listener it installed.
 */
export function bindInputEvents(network: Network, target: EventTarget): () => void;

/**
 * Initialize the shared wgpu (WebGPU) context against an HTML
 * `<canvas>`. Must be awaited once before any GPU-backed actor runs
 * — see [sdk/node#gpu-on-wasm](../README.md). On native runtimes the
 * argument is ignored and the GPU context is initialized lazily on
 * first use.
 *
 * @param canvasSelector CSS selector for the target `HTMLCanvasElement`,
 *   e.g. `"#viewport"`. Pass `null` for off-screen-only workloads.
 */
export function initGpuContext(canvasSelector?: string | null): Promise<void>;

/** Manifest payload of a `.rflpack`. Mirrors the on-disk JSON shape. */
export interface PackManifest {
  manifest_version: number;
  name: string;
  version: string;
  authors?: string[];
  description?: string;
  license?: string;
  reflow_pack_abi_version: number;
  entrypoint: string;
  targets: Record<string, { file: string }>;
  templates?: string[];
}

/** A loaded `.rflpack` ready to be wired into the runtime. */
export interface LoadedPack {
  manifest: PackManifest;
  name: string;
  version: string;
  templates: string[];
  wasm: Uint8Array;
  module: WebAssembly.Module;
}

/**
 * Fetch a `.rflpack` from a URL (typically a GitHub release asset),
 * verify its manifest, and compile its wasm32 entry into a
 * `WebAssembly.Module`. Native loaders ignore the wasm entry; this
 * is the browser-side equivalent.
 *
 * @param url Anything `fetch` accepts. GitHub release assets serve
 *   permissive CORS so cross-origin browser fetches work.
 */
export function loadPack(url: string | URL | Request): Promise<LoadedPack>;

/** Pack ABI version this runtime was built against. */
export function packAbiVersion(): number;

/**
 * `Message` payload constructors. The browser side serializes
 * messages as plain `{ type, data? }` JSON because the wasm runtime
 * round-trips through serde — there is no `ReflowMessage` class to
 * mirror. Use these helpers to keep parity with Node SDK code.
 */
export const Message: Readonly<{
  flow(): { type: "Flow" };
  boolean(v: boolean): { type: "Boolean"; data: boolean };
  integer(v: number): { type: "Integer"; data: number };
  float(v: number): { type: "Float"; data: number };
  string(v: string): { type: "String"; data: string };
  bytes(v: ArrayLike<number>): { type: "Bytes"; data: number[] };
  object(v: any): { type: "Object"; data: any };
  array(v: any): { type: "Array"; data: any };
  error(v: string): { type: "Error"; data: string };
  fromJson(v: any): any;
}>;

/**
 * Reflow `Graph` — Node-SDK-compatible browser wrapper. See
 * sdk/node/reflow.d.ts for the per-method docs; the contract is
 * identical except where browser-only behavior is documented inline.
 */
export class Graph {
  constructor(name?: string, caseSensitive?: boolean, properties?: any);

  /** Underlying wasm-bindgen `Graph` instance. */
  readonly raw: any;

  // Mutators
  addNode(id: string, component: string, metadata?: any): void;
  removeNode(id: string): void;
  renameNode(oldId: string, newId: string): void;
  setNodeMetadata(id: string, metadata: any): void;
  addConnection(
    outNode: string,
    outPort: string,
    inNode: string,
    inPort: string,
    metadata?: any,
  ): void;
  removeConnection(outNode: string, outPort: string, inNode: string, inPort: string): void;
  setConnectionMetadata(
    outNode: string,
    outPort: string,
    inNode: string,
    inPort: string,
    metadata: any,
  ): void;
  addInport(
    portId: string,
    nodeId: string,
    portKey: string,
    portType?: PortType | null,
    metadata?: any,
  ): void;
  removeInport(portId: string): void;
  renameInport(oldId: string, newId: string): void;
  setInportMetadata(portId: string, metadata: any): void;
  addOutport(
    portId: string,
    nodeId: string,
    portKey: string,
    portType?: PortType | null,
    metadata?: any,
  ): void;
  removeOutport(portId: string): void;
  renameOutport(oldId: string, newId: string): void;
  setOutportMetadata(portId: string, metadata: any): void;
  addGroup(groupId: string, nodes: string[], metadata?: any): void;
  removeGroup(groupId: string): void;
  addToGroup(groupId: string, nodeId: string): void;
  removeFromGroup(groupId: string, nodeId: string): void;
  setGroupMetadata(groupId: string, metadata: any): void;

  // IIPs (Node-SDK arg order)
  addInitial(node: string, port: string, data: any, metadata?: any): void;
  addInitialIndex(node: string, port: string, data: any, index: number, metadata?: any): void;
  addGraphInitial(inport: string, data: any, metadata?: any): void;
  addGraphInitialIndex(inport: string, data: any, index: number, metadata?: any): void;
  removeInitial(node: string, port: string): void;
  removeGraphInitial(inport: string): void;

  setProperties(properties: any): void;

  // Queries (Node-SDK getter names)
  getNode(id: string): GraphNode | undefined;
  getConnection(
    outNode: string,
    outPort: string,
    inNode: string,
    inPort: string,
  ): GraphConnection | undefined;
  nodes(): Record<string, GraphNode>;
  connections(): GraphConnection[];
  groups(): any;
  initializers(): any;
  properties(): any;
  inports(): Record<string, any>;
  outports(): Record<string, any>;

  // (de)serialization
  toJson(): GraphExport;
  toJSON(): GraphExport;
  static fromJson(obj: GraphExport, metadata?: any): Graph;
  static load(obj: GraphExport, metadata?: any): Graph;

  subscribe(callback: (event: any) => void): void;
}

/**
 * Authoring base class for browser actors. Same shape as the Node
 * SDK `Actor` — subclass, set static fields, override `run(ctx)`.
 */
export class Actor {
  static component: string;
  static inports: readonly string[];
  static outports: readonly string[];
  static awaitAllInports: boolean;

  run(ctx: ActorRunContext): void | Promise<void>;
}

/** Runtime context passed to `Actor#run`. */
export interface ActorRunContext {
  readonly input: Record<string, any>;
  readonly state: any;
  readonly config: any;
  send(messages: Record<string, any>): void;
  done(outputs?: Record<string, any>): void;
  fail(message: string): void;
}

/**
 * Reflow `Network` — Node-SDK-compatible browser wrapper. The
 * underlying wasm `GraphNetwork` is constructed from the accumulated
 * Graph state on `start()`.
 */
export class Network {
  /**
   * @param config - Optional. Pass a `Graph` to wrap an existing
   *   graph; pass nothing (or a config object) to start from an
   *   empty internal graph.
   */
  constructor(config?: Graph | object);

  static fromGraph(graph: Graph): Network;

  /** The Graph backing this Network. */
  readonly graph: Graph;
  /** Underlying wasm-bindgen `GraphNetwork` (after `start()`). */
  readonly raw: any;

  addNode(id: string, templateId: string, config?: any): void;
  addConnection(fromActor: string, fromPort: string, toActor: string, toPort: string): void;
  addInitial(actor: string, port: string, message: any): void;
  registerActor(templateId: string, actor: Actor | object): void;

  start(): Promise<void>;
  shutdown(): void;

  events(): EventStream;

  injectInputEvent(componentType: string, eventData: any): void;
  emit(actorId: string, packet: any): void;
  sendToActor(actorId: string, port: string, data: any): void;
  getActorCount(): number;
  getActorNames(): string[];
  getActiveActors(): string[];
}

/**
 * Async event source returned by `network.events()`. `.recv()`
 * resolves with the next event or `null` once the stream closes.
 */
export class EventStream {
  recv(): Promise<any | null>;
  close(): void;
}

declare const _default: {
  Graph: typeof Graph;
  Network: typeof Network;
  Actor: typeof Actor;
  Message: typeof Message;
  EventStream: typeof EventStream;
  bindInputEvents: typeof bindInputEvents;
  version: typeof version;
  ready: typeof ready;
  initGpuContext: typeof initGpuContext;
  loadPack: typeof loadPack;
  packAbiVersion: typeof packAbiVersion;
};
export default _default;
