# Reactive particle field in the browser

We are building a reactive particle field. Two hundred coloured
points spread across the canvas, each leaning toward the cursor with
its own spring physics. The cluster never collapses to a single dot
because every particle has a fixed home — the cursor only deforms a
local patch of the field. One HTML file, runs in any modern browser.

The animation has four jobs: pacing to the screen's frame rate,
reading the cursor, advancing each particle's physics one step, and
painting to the canvas. A vanilla implementation tangles them
together inside one `requestAnimationFrame` callback, with shared
mutable state for the particle array, the latest mouse position, and
the canvas context. Each new feature — record-and-replay, a second
renderer, a force field — has to thread through that callback.

Reflow gives each of those four jobs its own actor with declared
inports and outports, and the runtime calls each actor's `run(ctx)`
whenever a new packet lands on one of its inports. Swap the
canvas2d renderer for a WebGL one? Write a new actor with the same
inport, change one line in the wiring. Add recording? Insert a node
between simulator and renderer. The other actors never notice. The
data flow is explicit data, not buried inside a callback.

By the end of this tutorial you will have built the demo above and
understood the pattern well enough to read any other Reflow program.

<iframe src="../../embeds/tutorial-01-particle-field/" loading="lazy"
        style="width:100%;height:420px;border:1px solid #2a3045;border-radius:6px;background:#0b1020;"
        title="Live particle field demo"></iframe>

## What we are building

```mermaid
flowchart LR
    tick[clock] -->|dt + time| sim[simulate]
    mouse([mouse]) -->|position| sim
    sim -->|particles| draw[draw on canvas]
```

Three actors and one DOM event source. The `clock` actor fires once per
animation frame. `simulate` advances each particle one step. `draw`
paints them. The mouse position is read off `mousemove` events that we
inject straight into the graph.

If you have written a simulation before, this is the same loop you
always write. The shape is just declared instead of nested in
`requestAnimationFrame`.

## Setup

Pick any directory and put one file in it.

```html
<!doctype html>
<meta charset="utf-8">
<title>Particle field</title>
<style>
  body { margin: 0; background: #0b1020; color: #c9d2e6; font: 14px system-ui; }
  canvas { display: block; }
  small { position: fixed; bottom: 8px; left: 12px; opacity: .6; }
</style>
<canvas id="stage"></canvas>
<small>move your mouse</small>

<script type="module">
import { ready, Network, Actor, Message, bindInputEvents }
  from "https://esm.sh/@offbit-ai/reflow";

await ready();
// the rest goes here
</script>
```

The `esm.sh` URL fetches the browser build of `@offbit-ai/reflow` as an
ES module. `ready()` initialises the wasm runtime exactly once. Anything
below that line can use `Network`, `Actor`, `Message`.

## The actors

Three small classes. Read them top to bottom; the runtime calls
`run(ctx)` on each one whenever its inputs are ready.

### Clock

Fires every animation frame and emits the elapsed time since the
previous tick.

```js
class Clock extends Actor {
  static inports = ["tick"];
  static outports = ["tick", "dt", "time"];

  constructor() {
    super();
    this.last = performance.now();
  }

  run(ctx) {
    const now = performance.now();
    const dt = (now - this.last) / 1000;
    this.last = now;
    ctx.send({
      dt:   Message.float(dt),
      time: Message.float(now / 1000),
    });
    requestAnimationFrame(() => {
      ctx.send({ tick: Message.flow() });   // self-loop: re-fire next frame
      ctx.done();
    });
  }
}
```

The runtime fires `run` whenever a packet arrives on an inport. Since
the clock has no upstream, we wire its own `tick` outport back to its
`tick` inport (a self-loop, set up below) and seed the loop with one
initial packet. From then on the actor paces itself: each `run`
schedules one `requestAnimationFrame` callback, the callback emits a
fresh tick on the outport, the loop delivers it back, the runtime
calls `run` again. One pass per browser frame, no drift.

### Simulate

Holds the particle array. Each particle gets a fixed `home` position
across the canvas plus its own spring constants, so the field stays
distributed and every particle has a slightly different response. Each
tick the particle's effective target is its home pulled partway toward
the cursor — close particles bend hard, far particles barely move.

```js
const N = 200;
class Simulate extends Actor {
  static inports = ["dt", "mouse"];
  static outports = ["particles"];

  constructor(width, height) {
    super();
    this.target = { x: width / 2, y: height / 2 };
    this.particles = Array.from({ length: N }, () => {
      const hx = Math.random() * width;
      const hy = Math.random() * height;
      return {
        x: hx, y: hy, vx: 0, vy: 0,
        hx, hy,
        k: 6 + Math.random() * 4,        // stiffness 6–10 (1/sec²)
        c: 2.5 + Math.random() * 1.5,    // damping  2.5–4 (1/sec)
        color: `hsl(${Math.random() * 360}, 80%, 70%)`,
      };
    });
    this.influence = Math.min(width, height) * 0.4;
  }

  run(ctx) {
    const dt = Math.min(ctx.input.dt?.data ?? 0, 0.05);
    if (ctx.input.mouse) this.target = ctx.input.mouse.data;
    const r2 = this.influence * this.influence;
    for (const p of this.particles) {
      const dx = this.target.x - p.hx;
      const dy = this.target.y - p.hy;
      const lean = r2 / (r2 + dx * dx + dy * dy);
      const tx = p.hx + dx * lean;
      const ty = p.hy + dy * lean;
      const ax = (tx - p.x) * p.k - p.vx * p.c;
      const ay = (ty - p.y) * p.k - p.vy * p.c;
      p.vx += ax * dt;
      p.vy += ay * dt;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
    }
    ctx.send({ particles: Message.array(this.particles) });
    ctx.done();
  }
}
```

`ctx.input.dt` is the `Message` we sent from Clock — its `data` field
is the float. `ctx.input.mouse` may be absent on ticks the cursor
hasn't moved, which is why we keep `this.target` from the previous
one. Clamping `dt` to 0.05 stops a tab-switch hitch from blowing up
the integrator. The physics is plain underdamped spring + viscous
drag, units in seconds — frame-rate independent, so the demo behaves
the same on a 60Hz laptop as on a 240Hz desktop.

### Draw

Paints the particles onto the canvas.

```js
class Draw extends Actor {
  static inports = ["particles"];
  static outports = [];
  static portDelivery = { particles: "latest" };

  constructor(canvas) {
    super();
    this.ctx2d = canvas.getContext("2d");
    this.canvas = canvas;
  }

  run(ctx) {
    const ps = ctx.input.particles?.data ?? [];
    const c = this.ctx2d;
    c.fillStyle = "rgba(11, 16, 32, 0.35)";        // motion-blur trail
    c.fillRect(0, 0, this.canvas.width, this.canvas.height);
    for (const p of ps) {
      c.fillStyle = p.color;
      c.fillRect(p.x | 0, p.y | 0, 2, 2);
    }
    ctx.done();
  }
}
```

Two notes. The semi-transparent fill on every frame is what gives the
trails. And `static portDelivery = { particles: "latest" }` is a hint
to the runtime: the simulator can outpace the painter, so on the
`particles` inport keep only the freshest packet — drop older ones.
Without it, a slow `Draw` would build an inbox of stale particle
arrays.

## Wiring

Now we declare the graph and start it.

```js
const canvas = document.getElementById("stage");
canvas.width = innerWidth;
canvas.height = innerHeight;

const net = new Network();

net.addNode("clock", "tpl_clock");
net.addNode("mouse", "tpl_mouse_input");       // built-in DOM source
net.addNode("sim",   "tpl_simulate");
net.addNode("draw",  "tpl_draw");

net.addConnection("clock", "tick",       "clock", "tick");        // self-loop
net.addConnection("clock", "dt",         "sim",   "dt");
net.addConnection("mouse", "position",   "sim",   "mouse");
net.addConnection("sim",   "particles",  "draw",  "particles");

net.registerActor("tpl_clock",    new Clock());
net.registerActor("tpl_simulate", new Simulate(canvas.width, canvas.height));
net.registerActor("tpl_draw",     new Draw(canvas));

net.addInitial("clock", "tick", Message.flow());
await net.start();
bindInputEvents(net, document.body);
```

The `addInitial` line is what gets the clock running. It drops one
`Flow` packet onto the clock's `tick` inport; the runtime sees an
input ready, calls `run(ctx)` once, and the self-loop carries it from
there. No initial packet, no first tick, nothing moves.

`bindInputEvents` is called *after* `start()` because the runtime's
`GraphNetwork` is created lazily during start, and binding listeners
is what routes browser events into the matching input actor — here
`tpl_mouse_input`. It listens for `mousemove` (and a few other DOM
events; we only care about mousemove for this demo) and routes each
Reflow ships that template by default so we just connect it.

## Run it

Any static server works:

```sh
npx serve .
```

Open the page. Move the mouse. The particles follow.

## What Reflow gave us

A vanilla version of the same demo would be one big function with a
particle array, an event listener, a `requestAnimationFrame` callback,
and the drawing code intermingled. Three concerns in one place.

In the Reflow version each actor has one job and one set of inputs and
outputs. Swapping the renderer for a WebGL one means writing a new
`Draw` actor and changing one line in the graph. Adding a second
target, or a force field, or a record-and-replay layer, means adding a
node to the graph. Nothing that already works has to change.

That separation is why graphs are usually authored visually for
anything bigger than this. A 12-actor scene is hard to picture from
code, easy to read on a canvas.

## What is next

The next post takes the same ideas to Python. We will build a LangGraph
agent whose video-summary tool is a Reflow flow, and watch the actor
graph swallow the deterministic data work that does not belong inside
a prompt.

If you want to keep going on the browser side first, swap the `Draw`
actor in this tutorial for one that calls `tpl_sdf_render` from the
GPU pack. The same network, the same wiring, a different visual.
