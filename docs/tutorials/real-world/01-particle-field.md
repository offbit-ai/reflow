# Reactive particle field in the browser

A reactive particle field rendered to a `<canvas>`. 200 coloured
points spread across the screen and lean toward the cursor with
their own spring physics. Every particle has a fixed home, so the
field stays distributed; the cursor only deforms a local patch.
One HTML file, no install, runs in any modern browser.

The animation has four jobs: pacing to the screen's frame rate,
reading the cursor, advancing per-particle physics, painting. A
vanilla implementation packs all four into one
`requestAnimationFrame` callback with shared mutable state. Reflow
splits them into four actors with declared inports and outports.
The runtime calls each actor's `run(ctx)` when a new packet lands
on one of its inports. Replace the canvas2D renderer with WebGL by
writing a new `Draw` actor and changing one line in the wiring.
Insert a recording node between simulator and renderer without
touching the other actors.

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

Three actors and one DOM event source. `clock` fires once per
animation frame; `simulate` advances each particle one step; `draw`
paints. The mouse position comes from `mousemove` events injected
into the graph by `bindInputEvents`.

## Setup

One file in any directory:

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

`esm.sh` fetches the browser build of `@offbit-ai/reflow` as an ES
module. `ready()` initialises the wasm runtime once. After that
line, `Network`, `Actor`, and `Message` are available.

## The actors

### Clock

Fires once per animation frame, emits `dt` and `time` on each tick.

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

The clock has no upstream actor, so we wire its own `tick` outport
back to its `tick` inport (a self-loop, set up below) and seed the
loop with one initial packet. Each `run` schedules one
`requestAnimationFrame` callback; the callback emits a fresh tick
on the outport, the loop delivers it back, the runtime calls `run`
again. One pass per browser frame, no drift.

### Simulate

Holds the particle array. Each particle has a fixed `home` position
plus its own spring constants and colour. The effective target each
tick is `home + (cursor − home) · lean`, where `lean` falls off
with distance from the cursor — close particles bend hard, far
particles barely move.

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

`ctx.input.dt` is the `Message` Clock sent; its `.data` is the
float. `ctx.input.mouse` is absent on ticks where the cursor
didn't move, so we cache `this.target`. Clamping `dt` to 0.05
stops a tab-switch hitch from blowing up the integrator. The
physics is underdamped spring + viscous drag in seconds — same
behaviour at 60Hz and 240Hz.

### Draw

Paints particles to the canvas.

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

The semi-transparent fill each frame produces the motion-blur
trails. `static portDelivery = { particles: "latest" }` tells the
runtime that the simulator can outpace the painter — keep only the
freshest packet on `particles`, drop stale ones. Without it, a slow
`Draw` builds an inbox of unused particle arrays.

## Wiring

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

`addInitial` drops one `Flow` packet on the clock's `tick` inport.
The runtime calls `run(ctx)` once, the self-loop carries it from
there. Without this line nothing fires.

`bindInputEvents` is called after `start()` — the wasm
`GraphNetwork` is created lazily during start. It attaches DOM
listeners (mousemove, keydown, etc.) and routes events into the
matching built-in input actor. `tpl_mouse_input` ships with the
catalog, so the wiring is just `mouse.position → sim.mouse`.

## Run it

Any static server works:

```sh
npx serve .
```

Open the page. Move the mouse. The particles follow.

## Notes on the design

- Each actor has one job and a single set of inports and outports.
  Replace the renderer (canvas2D → WebGL) by writing a new `Draw`
  actor and changing one line in the wiring.
- Add a recording layer or a force field by inserting a node
  between simulator and renderer. The other actors don't need to
  change.
- Larger graphs (12+ actors) are typically authored in
  [Zeal](https://github.com/offbit-ai/zeal) and loaded from JSON
  rather than wired in code.

## What is next

The next tutorial moves to Python and uses Reflow as a multi-agent
orchestrator: three LLM agents run in parallel against a local
Ollama model and a synthesizer combines their findings.
