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

// ─── Pack loading ──────────────────────────────────────────────────────────

/**
 * Load a `.rflpack` from a URL, extract its `wasm32-unknown-unknown`
 * binary, compile it, and (optionally) register every template it
 * publishes against a running `Network`.
 *
 * Designed to point straight at a GitHub release asset:
 *
 *     await ready();
 *     const net = new Network();
 *     const pack = await loadPack(
 *       "https://github.com/offbit-ai/reflow/releases/download/" +
 *       "pack-v0.2/reflow.pack.gpu-0.2.0.rflpack",
 *       { network: net },
 *     );
 *     console.log(pack.registered); // ["tpl_sdf_render", ...]
 *
 * `url` accepts anything `fetch` does — a string URL, a `URL`, or
 * a pre-built `Request`. GitHub release assets serve permissive
 * CORS headers so cross-origin browser fetches just work.
 *
 * **Smart URL rewriting.** First-party releases ship two flavours
 * of every pack: a full multi-triple `.rflpack` (~22 MiB) and a
 * `<name>-<version>-wasm32-unknown-unknown.rflpack` slim (~1.8 MiB)
 * that contains only the browser binary. Browsers never need
 * anything but the wasm entry, so when the URL points at the full
 * bundle we transparently try the wasm slim first and fall back
 * to the URL the user passed if it 404s. Pass
 * `options.preferFullBundle: true` to opt out — useful when you've
 * republished a custom slim under a non-standard filename.
 *
 * The optional `options.network` triggers the pack-ABI handshake:
 *   1. Compile + instantiate the wasm with the
 *      `env.__reflow_pack_register_template` import wired to a
 *      callback that captures `(name, factoryId)` pairs as the pack
 *      walks its `#[reflow_pack]` register function.
 *   2. Call the pack's exported `__reflow_pack_register()`. Each
 *      `host.register("name", factory)` inside the pack fires our
 *      import callback once.
 *   3. For every captured pair, register a JS adapter actor with
 *      `network` whose `run(ctx)` calls back into
 *      `instance.exports.__reflow_pack_create_actor(factoryId)`.
 *
 * **Status of the actor adapter (TBD).** The pack-side
 * `__reflow_pack_create_actor(id)` returns a `*mut PackActorHandle`
 * — a pointer into the pack's wasm linear memory. The runtime
 * lives in a separate wasm module with a separate memory, so a raw
 * pointer can't cross the boundary. Wiring full message-passing
 * across pack ↔ runtime needs a serialization protocol over the
 * pack's exported memory; that's the next milestone. For now the
 * adapter calls `__reflow_pack_create_actor`, leaks the returned
 * pointer (the pack tracks it internally), and `ctx.fail`s with a
 * clear "wasm pack actor execution not yet wired" message. The
 * registration plumbing is still useful: it proves the
 * import/export handshake works and `network.getActorNames()`
 * reflects the loaded templates.
 *
 * Returns `{ manifest, name, version, templates, wasm, module,
 * instance, registered }`. `instance` is `null` if no network was
 * provided — the caller can call `attachToNetwork(network)` on the
 * returned pack later.
 *
 * Throws if the URL fetch fails, the pack lacks a wasm32 build,
 * its `reflow_pack_abi_version` doesn't match the runtime's, or
 * the pack-side register function returns non-zero.
 */
export async function loadPack(url, options = {}) {
  // The browser only ever runs the wasm32 entry. If the URL points
  // at a full multi-triple `.rflpack` (~22 MiB), we transparently
  // try the wasm-only slim variant first
  // (`<name>-<version>-wasm32-unknown-unknown.rflpack`, ~1.8 MiB)
  // and fall back to the URL the user gave if that 404s. Set
  // `options.preferFullBundle` to skip the optimization.
  const candidates = [];
  if (!options.preferFullBundle) {
    const slim = wasmSlimUrl(url);
    if (slim) candidates.push(slim);
  }
  candidates.push(url);

  let response;
  let lastError;
  for (const candidate of candidates) {
    try {
      const r = await fetch(candidate);
      if (r.ok) {
        response = r;
        break;
      }
      lastError = `${r.status} ${r.statusText}`;
    } catch (err) {
      lastError = String(err);
    }
  }
  if (!response) {
    throw new Error(`loadPack: fetch ${url} → ${lastError ?? "unreachable"}`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());

  // wasm-bindgen helper: validates manifest_version + ABI, then
  // returns the embedded wasm bytes. Throws a JS Error string on
  // any mismatch.
  const extracted = wasm.extractPackWasm(bytes);
  const manifest = JSON.parse(extracted.manifestJson);
  const wasmBytes = extracted.wasm;
  const module = await WebAssembly.compile(wasmBytes);

  const pack = {
    manifest,
    name: manifest.name,
    version: manifest.version,
    templates: manifest.templates ?? [],
    wasm: wasmBytes,
    module,
    instance: null,
    registered: [],

    /**
     * Instantiate the pack and register its templates with `network`.
     * Idempotent: a second call returns the existing registration list.
     */
    async attachTo(network) {
      if (this.instance) return this.registered;
      const result = await attachPackToNetwork(this.module, network);
      this.instance = result.instance;
      this.registered = result.registered;
      return this.registered;
    },
  };

  if (options.network) {
    await pack.attachTo(options.network);
  }
  return pack;
}

// Internal: derive the wasm-only slim URL from a full bundle URL.
// Returns null if `url` doesn't end in `.rflpack` or already names a
// triple slim (we don't double-rewrite).
const KNOWN_TRIPLES = [
  "wasm32-unknown-unknown",
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
];
function wasmSlimUrl(url) {
  const s = String(url);
  if (!s.endsWith(".rflpack")) return null;
  // Already a slim variant for some triple — leave it alone. The
  // user is being explicit; respect that.
  for (const triple of KNOWN_TRIPLES) {
    if (s.endsWith(`-${triple}.rflpack`)) return null;
  }
  return s.slice(0, -".rflpack".length) + "-wasm32-unknown-unknown.rflpack";
}

// Internal: walk the pack's register fn and wire it to the network.
// Split out so `attachTo` can also call it post-hoc.
async function attachPackToNetwork(module, network) {
  /** Captured at registration time:
   *   { name: string, factoryId: number, inports: string[], outports: string[] }
   */
  const registered = [];

  const decoder = new TextDecoder();
  const encoder = new TextEncoder();

  // Helper: build a fresh view over the pack's current linear memory
  // (the buffer can be detached after a wasm grow, so re-fetch each
  // time rather than caching `Uint8Array`).
  const memBytes = () => new Uint8Array(instance.exports.memory.buffer);
  const memView = () => new DataView(instance.exports.memory.buffer);

  const importObject = {
    env: {
      // Fired once per `host.register("name", factory)` inside the
      // pack's `#[reflow_pack]` function. The metadata blob is JSON
      // `{name, inports, outports}` so the adapter can declare the
      // right ports without a second round-trip.
      __reflow_pack_register_template: (metaPtr, metaLen, factoryId) => {
        const view = memBytes().subarray(metaPtr, metaPtr + metaLen);
        const meta = JSON.parse(decoder.decode(view));
        registered.push({
          name: meta.name,
          factoryId,
          inports: Array.isArray(meta.inports) ? meta.inports : [],
          outports: Array.isArray(meta.outports) ? meta.outports : [],
        });
      },
    },
  };

  // `instance` is captured by the closures above. The
  // import callback only fires from inside the explicit
  // `__reflow_pack_register()` call below, so the binding is set
  // by then.
  let instance;
  ({ instance } = await WebAssembly.instantiate(module, importObject));

  const status = instance.exports.__reflow_pack_register();
  if (status !== 0) {
    throw new Error(`pack __reflow_pack_register returned status ${status}`);
  }

  for (const { name, factoryId, inports, outports } of registered) {
    // One pack-side actor instance per template registration. The
    // pack maintains the instance in its static `INSTANCES` table.
    // We treat the returned `instanceId` as opaque.
    const instanceId = instance.exports.__reflow_pack_create_actor(factoryId);
    if (instanceId === 0) {
      throw new Error(
        `pack __reflow_pack_create_actor(${factoryId}) returned 0 — ` +
          `factory id out of range`,
      );
    }

    network.registerActor(name, {
      inports,
      outports,
      run(ctx) {
        const out = invokePackActor(instance, instanceId, ctx.input ?? {});
        if (out.ok) {
          ctx.send(out.outputs);
          ctx.done();
        } else {
          ctx.fail(out.error);
        }
      },
    });
  }

  return { instance, registered };
}

/**
 * One tick of execution against a pack-side actor instance.
 *
 * Wire shape (pack ABI v1, sync execution only):
 *   1. JSON-encode the input port map.
 *   2. `__reflow_pack_alloc` enough bytes in pack memory; copy in.
 *   3. `__reflow_pack_alloc(8)` for two output slots (ptr, len).
 *   4. `__reflow_pack_actor_run(instance_id, in_ptr, in_len,
 *       out_ptr_slot, out_len_slot)` returns 0 on success.
 *   5. Read the result ptr+len, slice out the JSON, decode.
 *   6. `__reflow_pack_free` everything.
 *
 * Async actors (those that `.await` JS Promises like fetch / wgpu
 * map_async) currently hang the pack-side `pollster::block_on`. The
 * follow-up milestone integrates wasm-bindgen-futures so async
 * packs can yield to the JS event loop.
 *
 * @returns `{ ok: true, outputs }` or `{ ok: false, error }`
 */
function invokePackActor(instance, instanceId, inputMap) {
  const exp = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  const payloadJson = JSON.stringify({ input: inputMap });
  const payloadBytes = encoder.encode(payloadJson);
  const payloadLen = payloadBytes.length;

  // Allocate input buffer + 8-byte slot pair for the output ptr/len.
  const payloadPtr = exp.__reflow_pack_alloc(payloadLen || 1);
  const outBlock = exp.__reflow_pack_alloc(8);
  if (!payloadPtr || !outBlock) {
    return { ok: false, error: "pack OOM during alloc" };
  }

  try {
    // Copy payload into pack memory.
    new Uint8Array(exp.memory.buffer, payloadPtr, payloadLen).set(payloadBytes);

    const status = exp.__reflow_pack_actor_run(
      instanceId,
      payloadPtr,
      payloadLen,
      outBlock,
      outBlock + 4,
    );

    const dv = new DataView(exp.memory.buffer);
    const resultPtr = dv.getUint32(outBlock, true);
    const resultLen = dv.getUint32(outBlock + 4, true);
    let resultJson = "";
    if (resultPtr && resultLen) {
      const view = new Uint8Array(exp.memory.buffer, resultPtr, resultLen);
      // .slice() detaches from the live wasm buffer so a later
      // grow() doesn't invalidate our reference.
      resultJson = decoder.decode(view.slice());
      exp.__reflow_pack_free(resultPtr, resultLen);
    }

    if (status !== 0) {
      let msg = `pack actor run failed (status ${status})`;
      try {
        const parsed = JSON.parse(resultJson);
        if (parsed?.error) msg = parsed.error;
      } catch {
        /* keep default msg */
      }
      return { ok: false, error: msg };
    }

    return { ok: true, outputs: resultJson ? JSON.parse(resultJson) : {} };
  } finally {
    exp.__reflow_pack_free(payloadPtr, payloadLen || 1);
    exp.__reflow_pack_free(outBlock, 8);
  }
}

// ─── Pass-through wasm exports ─────────────────────────────────────────────

export const bindInputEvents = wasm.bindInputEvents;
export const version = wasm.version;

/** Pack ABI version this runtime was built against. Diagnostic only. */
export const packAbiVersion = wasm.packAbiVersion;

/**
 * Initialize the shared wgpu context against an HTML `<canvas>`.
 *
 * Must be awaited once during workflow startup before any GPU-backed
 * actor runs. Browser-side wgpu uses the WebGPU backend (with WebGL2
 * fallback in older browsers); the canvas registration matters because
 * Chromium's WebGPU implementation refuses to hand out a
 * presentation-capable adapter without a target surface.
 *
 *     await ready();
 *     await initGpuContext("#viewport");
 *     // ...build network and start
 *
 * Pass `null` (or omit) for off-screen workloads (mesh ops, SDF
 * readback) where the result is read back to CPU and displayed
 * outside the wgpu pipeline. On native runtimes the argument is
 * ignored and the GPU context is initialized lazily on first use.
 */
export const initGpuContext = wasm.initGpuContext;

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
  initGpuContext,
  loadPack,
  packAbiVersion,
};
