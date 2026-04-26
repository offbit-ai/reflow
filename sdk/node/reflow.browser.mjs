// Browser entrypoint for @offbit-ai/reflow.
//
// In browsers, the napi `.node` addon can't load (no Node process,
// no native loader). The "browser" conditional export in
// package.json routes `import { Graph, Network } from
// "@offbit-ai/reflow"` to this file, which serves the wasm-bindgen
// build under `./wasm/` instead.
//
// This module wraps the wasm classes in a thin JS shim so the
// browser surface matches the Node SDK 1:1 — same constructor
// shape, same method names, same argument order. Where the
// wasm-bindgen output already aligns (most of `Graph`), we
// pass through; where it doesn't (param order on add*Initial,
// getter naming, the Network constructor / events() pattern), we
// translate.
//
// The underlying wasm instance is reachable via `.raw` on every
// wrapper if you need the native shape for some reason.

import init, * as wasm from "./wasm/reflow_rt_wasm.js";

// ─── GraphExport shape conversion ──────────────────────────────────────────
//
// wasm-bindgen serializes Rust `HashMap<K, V>` as `Map<K, V>` in JS.
// The napi-rs Node SDK serializes the same Rust types as plain
// `{key: value}` objects. Both shapes describe the same data; we
// normalize to plain-object shape on every public boundary so users
// don't have to care which backend they're talking to.
//
// Fields that need conversion live on `GraphExport` (processes,
// inports, outports, properties, providedInterfaces) and on the
// per-node `metadata` field. The conversion is shallow on the
// containers but recursive into individual entries (a node's
// metadata can itself be a Map<string, any>).

const MAP_FIELDS = ["processes", "inports", "outports", "properties", "providedInterfaces"];

function mapToObject(map) {
  if (map == null) return map;
  if (map instanceof Map) {
    const out = {};
    for (const [k, v] of map) out[k] = mapToObjectDeep(v);
    return out;
  }
  return map;
}

function mapToObjectDeep(value) {
  if (value instanceof Map) return mapToObject(value);
  if (Array.isArray(value)) return value.map(mapToObjectDeep);
  if (value && typeof value === "object") {
    const out = {};
    for (const k of Object.keys(value)) out[k] = mapToObjectDeep(value[k]);
    return out;
  }
  return value;
}

function mapsToObjects(graphExport) {
  if (graphExport == null) return graphExport;
  const out = { ...graphExport };
  for (const k of MAP_FIELDS) {
    if (out[k] !== undefined) out[k] = mapToObject(out[k]);
  }
  // Per-node `metadata` and per-connection `metadata` may also be
  // Map<string, any> — walk them.
  if (out.processes && typeof out.processes === "object") {
    for (const [id, node] of Object.entries(out.processes)) {
      if (node?.metadata instanceof Map) {
        out.processes[id] = { ...node, metadata: mapToObject(node.metadata) };
      }
    }
  }
  if (Array.isArray(out.connections)) {
    out.connections = out.connections.map((c) =>
      c?.metadata instanceof Map ? { ...c, metadata: mapToObject(c.metadata) } : c,
    );
  }
  return out;
}

function objectToMap(obj) {
  if (obj == null) return obj;
  if (obj instanceof Map) return obj;
  return new Map(Object.entries(obj));
}

function objectsToMaps(graphExport) {
  if (graphExport == null) return graphExport;
  const out = { ...graphExport };
  for (const k of MAP_FIELDS) {
    if (out[k] !== undefined && !(out[k] instanceof Map)) {
      out[k] = objectToMap(out[k]);
    }
  }
  return out;
}

// ─── Init ──────────────────────────────────────────────────────────────────

let _ready;

/**
 * Initialize the wasm module. Call once before constructing Graph
 * or Network. Subsequent calls return the cached promise so it's
 * safe to await from multiple call sites.
 *
 * @param {RequestInfo|URL|BufferSource|WebAssembly.Module} [moduleOrUrl]
 *   Optional override for where to fetch the .wasm.
 */
export function ready(moduleOrUrl) {
  if (!_ready) _ready = init(moduleOrUrl);
  return _ready;
}

// ─── Message ───────────────────────────────────────────────────────────────

// The wasm side doesn't ship a `Message` class — it accepts the raw
// `{type, data}` JSON shape directly. We mirror the Node SDK's
// helper set so isomorphic code can write `Message.integer(42)`
// regardless of target. The returned objects are plain JSON; the
// runtime deserializes them on the Rust side.

/**
 * Constructors for Reflow `Message` payloads. Mirrors the Node SDK
 * `Message` static methods.
 */
export const Message = Object.freeze({
  flow: () => ({ type: "Flow" }),
  boolean: (v) => ({ type: "Boolean", data: !!v }),
  integer: (v) => ({ type: "Integer", data: v | 0 }),
  float: (v) => ({ type: "Float", data: Number(v) }),
  string: (v) => ({ type: "String", data: String(v) }),
  bytes: (v) => ({ type: "Bytes", data: Array.from(v) }),
  object: (v) => ({ type: "Object", data: v }),
  array: (v) => ({ type: "Array", data: v }),
  error: (v) => ({ type: "Error", data: String(v) }),
  fromJson: (v) => v,
});

// ─── Graph ─────────────────────────────────────────────────────────────────

/**
 * Reflow `Graph` — Node-SDK-compatible wrapper over the wasm
 * `Graph` class.
 *
 * Differences vs the raw wasm class that this wrapper smooths over:
 *   - `addInitial(node, port, data, metadata?)` — Node order. The
 *     wasm side takes `(data, node, port, metadata)`.
 *   - `addInitialIndex`, `addGraphInitial`, `addGraphInitialIndex`
 *     — same param-order shift.
 *   - Getters: `nodes()` / `connections()` / `groups()` / etc.
 *     The wasm exposes `getNodes()` / `getConnections()` / ….
 *   - `toJson()` ↔ wasm `toJSON()`.
 */
export class Graph {
  /**
   * @param {string} [name]
   * @param {boolean} [caseSensitive]
   * @param {object} [properties]
   */
  constructor(name = "", caseSensitive = false, properties = null) {
    if (name && typeof name === "object" && name instanceof wasm.Graph) {
      // Internal: wrap an existing wasm Graph (used by Graph.load).
      this._inner = name;
    } else {
      this._inner = new wasm.Graph(name, caseSensitive, properties);
    }
  }

  /** Underlying wasm-bindgen `Graph` instance (escape hatch). */
  get raw() {
    return this._inner;
  }

  // ─── Mutators (already-aligned naming, pass-through) ─────────────────────

  addNode(id, component, metadata = null) {
    this._inner.addNode(id, component, metadata);
  }
  removeNode(id) {
    this._inner.removeNode(id);
  }
  renameNode(oldId, newId) {
    this._inner.renameNode(oldId, newId);
  }
  setNodeMetadata(id, metadata) {
    this._inner.setNodeMetadata(id, metadata);
  }

  addConnection(outNode, outPort, inNode, inPort, metadata = null) {
    this._inner.addConnection(outNode, outPort, inNode, inPort, metadata);
  }
  removeConnection(outNode, outPort, inNode, inPort) {
    this._inner.removeConnection(outNode, outPort, inNode, inPort);
  }
  setConnectionMetadata(outNode, outPort, inNode, inPort, metadata) {
    this._inner.setConnectionMetadata(outNode, outPort, inNode, inPort, metadata);
  }

  addInport(portId, nodeId, portKey, portType = null, metadata = null) {
    this._inner.addInport(portId, nodeId, portKey, portType, metadata);
  }
  removeInport(portId) {
    this._inner.removeInport(portId);
  }
  renameInport(oldId, newId) {
    this._inner.renameInport(oldId, newId);
  }
  setInportMetadata(portId, metadata) {
    this._inner.setInportMetadata(portId, metadata);
  }

  addOutport(portId, nodeId, portKey, portType = null, metadata = null) {
    this._inner.addOutport(portId, nodeId, portKey, portType, metadata);
  }
  removeOutport(portId) {
    this._inner.removeOutport(portId);
  }
  renameOutport(oldId, newId) {
    this._inner.renameOutport(oldId, newId);
  }
  setOutportMetadata(portId, metadata) {
    this._inner.setOutportMetadata(portId, metadata);
  }

  addGroup(groupId, nodes, metadata = null) {
    this._inner.addGroup(groupId, nodes, metadata);
  }
  removeGroup(groupId) {
    this._inner.removeGroup(groupId);
  }
  addToGroup(groupId, nodeId) {
    this._inner.addToGroup(groupId, nodeId);
  }
  removeFromGroup(groupId, nodeId) {
    this._inner.removeFromGroup(groupId, nodeId);
  }
  setGroupMetadata(groupId, metadata) {
    this._inner.setGroupMetadata(groupId, metadata);
  }

  // ─── Initial Information Packets — argument order shifted ────────────────

  /**
   * Node-style: `addInitial(node, port, data, metadata?)`.
   * The wasm class takes `(data, node, port, metadata)`.
   */
  addInitial(node, port, data, metadata = null) {
    this._inner.addInitial(data, node, port, metadata);
  }

  addInitialIndex(node, port, data, index, metadata = null) {
    this._inner.addInitialIndex(data, node, port, index, metadata);
  }

  /**
   * Node-style: `addGraphInitial(inport, data, metadata?)`.
   * The wasm parameter is named `node` but semantically this is a
   * graph-level inport (top-level entry into the network).
   */
  addGraphInitial(inport, data, metadata = null) {
    this._inner.addGraphInitial(data, inport, metadata);
  }

  addGraphInitialIndex(inport, data, index, metadata = null) {
    this._inner.addGraphInitialIndex(data, inport, index, metadata);
  }

  removeInitial(node, port) {
    this._inner.removeInitial(node, port);
  }
  removeGraphInitial(inport) {
    this._inner.removeGraphInitial(inport);
  }

  // ─── Properties ──────────────────────────────────────────────────────────

  setProperties(properties) {
    this._inner.setProperties(properties);
  }

  // ─── Queries — Node uses bare-noun getters, wasm prefixes `get` ──────────

  getNode(id) {
    return this._inner.getNode(id);
  }
  getConnection(outNode, outPort, inNode, inPort) {
    return this._inner.getConnection(outNode, outPort, inNode, inPort);
  }
  nodes() {
    return this._inner.getNodes();
  }
  connections() {
    return this._inner.getConnections();
  }
  groups() {
    return this._inner.getGroups();
  }
  initializers() {
    return this._inner.getInitializers();
  }
  properties() {
    return this._inner.getProperties();
  }

  // The wasm Graph doesn't expose explicit inports()/outports()
  // readers — they live inside the JSON export. Pull them off the
  // (Map→object-converted) export so isomorphic code stays uniform.
  inports() {
    return this.toJson().inports ?? {};
  }
  outports() {
    return this.toJson().outports ?? {};
  }

  // ─── (de)serialization ───────────────────────────────────────────────────

  /**
   * The wasm-side `GraphExport` returns Maps for `processes`,
   * `inports`, `outports`, `properties`, … The Node SDK returns
   * plain objects there. We convert Maps→objects on the way out
   * and objects→Maps on the way in so isomorphic code can rely on
   * one shape.
   */
  toJson() {
    return mapsToObjects(this._inner.toJSON());
  }
  toJSON() {
    return this.toJson();
  }

  static fromJson(obj, metadata = null) {
    return new Graph(wasm.Graph.load(objectsToMaps(obj), metadata));
  }
  static load(obj, metadata = null) {
    return Graph.fromJson(obj, metadata);
  }

  /** Subscribe to the graph's mutation events. */
  subscribe(callback) {
    return this._inner.subscribe(callback);
  }
}

// ─── Actor ─────────────────────────────────────────────────────────────────

/**
 * Base class for browser-authored actors. Mirrors the Node SDK
 * `Actor` shape exactly: subclass, set `static component`, declare
 * `static inports` / `static outports`, override `run(ctx)`.
 *
 *   class Doubler extends Actor {
 *     static component = "doubler";
 *     static inports = ["in"];
 *     static outports = ["out"];
 *     run(ctx) { ctx.send({ out: Message.integer(2 * ctx.input.in.data) }); ctx.done(); }
 *   }
 *   net.registerActor("tpl_doubler", new Doubler());
 */
export class Actor {
  static component = "anonymous";
  static inports = [];
  static outports = [];
  static awaitAllInports = false;

  /**
   * Override in subclasses. Receive a context with `input`, `state`,
   * `config`, `send(messages)`. Resolve the tick by calling
   * `ctx.done(outputs?)` or `ctx.fail(message)` exactly once.
   *
   * May be async — throws / rejections auto-fail the tick.
   */
  run(_ctx) {
    throw new Error(`${this.constructor.name}.run() is not implemented`);
  }

  /**
   * Build the plain JS object the wasm runtime expects from
   * `network.registerActor(name, ...)`.
   *
   * Mirrors `Actor#_build()` on the Node side.
   *
   * @internal
   */
  _build() {
    const cls = this.constructor;
    const self = this;
    const inports = Array.from(cls.inports ?? []);
    const outports = Array.from(cls.outports ?? []);

    return {
      inports,
      outports,
      // wasm `Actor.state` getter/setter — wired by the runtime
      // when it constructs the per-instance ActorRunContext, so we
      // leave them unset on the prototype object.
      run(context) {
        let finished = false;
        const ctx = {
          get input() {
            return context.input;
          },
          get state() {
            return context.state;
          },
          get config() {
            return context.config;
          },
          send(messages) {
            context.send(normalizeOutputs(messages));
          },
          done(outputs) {
            if (finished) return;
            finished = true;
            if (outputs != null) {
              context.send(normalizeOutputs(outputs));
            }
          },
          fail(message) {
            if (finished) return;
            finished = true;
            // The wasm runtime treats throws inside run() as failures.
            throw new Error(String(message ?? "actor failed"));
          },
        };

        try {
          const result = self.run(ctx);
          if (result && typeof result.then === "function") {
            return result.catch((err) => ctx.fail(err?.stack || err?.message || String(err)));
          }
        } catch (err) {
          ctx.fail(err?.stack || err?.message || String(err));
        }
      },
    };
  }
}

function normalizeOutputs(outputs) {
  if (outputs == null) return {};
  if (typeof outputs !== "object" || Array.isArray(outputs)) {
    throw new TypeError("send/done expects an object keyed by port");
  }
  // Pass through plain Message JSON unchanged. (Node uses .asJson() on
  // its Message class; the browser Message namespace already returns
  // plain JSON, so there's nothing to unwrap here.)
  return outputs;
}

// ─── Network ───────────────────────────────────────────────────────────────

/**
 * Reflow `Network` — Node-SDK-compatible wrapper over the wasm
 * `GraphNetwork`.
 *
 * The wasm class is `Graph`-first: `new GraphNetwork(graph)`. We
 * preserve the Node-SDK constructor shape too — `new Network()`
 * builds an empty internal Graph that the imperative `addNode` /
 * `addConnection` / `addInitial` calls populate, then `start()`
 * realizes the wasm GraphNetwork and kicks off execution.
 */
export class Network {
  /**
   * @param {object|Graph} [config]
   *   - omitted / plain config object: builds an empty internal Graph
   *   - a `Graph` instance: wraps it (same as `Network.fromGraph(g)`)
   */
  constructor(config = null) {
    this._started = false;
    this._inner = null; // wasm.GraphNetwork — built lazily in start()
    this._eventStream = null;

    if (config instanceof Graph) {
      this._graph = config;
    } else {
      this._graph = new Graph();
      this._config = config;
    }
  }

  /** Construct a Network from an existing Graph. */
  static fromGraph(graph) {
    return new Network(graph);
  }

  /** The Graph backing this Network. Mutations propagate at start time. */
  get graph() {
    return this._graph;
  }

  /** Underlying wasm-bindgen `GraphNetwork`, valid after `start()`. */
  get raw() {
    return this._inner;
  }

  // ─── Pre-start graph mutations ───────────────────────────────────────────

  addNode(id, templateId, config = null) {
    this._graph.addNode(id, templateId, config);
  }

  addConnection(fromActor, fromPort, toActor, toPort) {
    this._graph.addConnection(fromActor, fromPort, toActor, toPort);
  }

  /**
   * Node-style `addInitial(actor, port, message)`. Routes through
   * the underlying Graph's `addInitial` (which we already shifted
   * to Node order).
   */
  addInitial(actor, port, message) {
    this._graph.addInitial(actor, port, message);
  }

  registerActor(templateId, actor) {
    if (actor == null) {
      throw new Error("registerActor(templateId, actor) requires two arguments");
    }
    const built = actor instanceof Actor ? actor._build() : actor;
    this._pendingActors ??= [];
    this._pendingActors.push([templateId, built]);
  }

  // ─── Lifecycle ───────────────────────────────────────────────────────────

  /**
   * Realize the wasm GraphNetwork from the accumulated Graph state,
   * register any actors queued via `registerActor`, and start
   * execution. Returns the `start()` Promise from wasm-bindgen.
   */
  start() {
    if (!this._started) {
      this._inner = new wasm.GraphNetwork(this._graph.raw);
      for (const [templateId, actor] of this._pendingActors ?? []) {
        this._inner.registerActor(templateId, actor);
      }
      this._started = true;
    }
    return this._inner.start();
  }

  shutdown() {
    if (this._inner) this._inner.shutdown();
  }

  // ─── Events — bridge wasm's callback API to Node's async recv() ──────────

  /**
   * Subscribe to network events. Returns an `EventStream` whose
   * `recv()` awaits the next event — same shape as Node's
   * `network.events()`.
   *
   * Internally, wasm exposes `next(callback)` which fires per
   * event. We queue events and resolve any pending `recv()` Promise.
   */
  events() {
    if (!this._inner) {
      throw new Error("events() requires the network to be started — call start() first");
    }
    if (!this._eventStream) {
      this._eventStream = new EventStream(this._inner);
    }
    return this._eventStream;
  }

  // ─── Pass-through escape hatches (wasm-only API surface) ─────────────────

  /** Inject a DOM-style input event. Same as on the wasm class. */
  injectInputEvent(componentType, eventData) {
    this._inner?.injectInputEvent(componentType, eventData);
  }

  emit(actorId, packet) {
    this._inner?.emit(actorId, packet);
  }

  sendToActor(actorId, port, data) {
    this._inner?.sendToActor(actorId, port, data);
  }

  getActorCount() {
    return this._inner?.getActorCount() ?? 0;
  }

  getActorNames() {
    return this._inner?.getActorNames() ?? [];
  }

  getActiveActors() {
    return this._inner?.getActiveActors() ?? [];
  }
}

/**
 * Async event source returned from `network.events()`. Wraps the
 * wasm `next(callback)` API behind a Promise-based `recv()` that
 * matches the Node SDK's `EventStream`.
 */
export class EventStream {
  constructor(graphNetwork) {
    this._queue = [];
    this._pending = null;
    this._closed = false;
    graphNetwork.next((event) => {
      if (this._pending) {
        const resolve = this._pending;
        this._pending = null;
        resolve(event);
      } else {
        this._queue.push(event);
      }
    });
  }

  /** Await the next event. Resolves `null` once the stream is closed. */
  recv() {
    if (this._queue.length > 0) {
      return Promise.resolve(this._queue.shift());
    }
    if (this._closed) return Promise.resolve(null);
    return new Promise((resolve) => {
      this._pending = resolve;
    });
  }

  /** Close the stream — pending `recv()` calls resolve to `null`. */
  close() {
    this._closed = true;
    if (this._pending) {
      const resolve = this._pending;
      this._pending = null;
      resolve(null);
    }
  }
}

// ─── Pass-through wasm exports ─────────────────────────────────────────────

export const bindInputEvents = wasm.bindInputEvents;
export const version = wasm.version;

// ─── Default export ────────────────────────────────────────────────────────

export default {
  Graph,
  Network,
  Actor,
  Message,
  EventStream,
  bindInputEvents,
  version,
  ready,
};
