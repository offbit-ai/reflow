# Input / Window Event Actors

Actors behind the `window-events` feature of `reflow_components` / `reflow_rt`. These turn OS-level input events into Reflow packets suitable for interactive graphs.

```toml
reflow_rt = { version = "0.1", features = ["window-events"] }
```

## What's included

| Template | Purpose |
|----------|---------|
| `tpl_keyboard_input` | Keyboard key-down / key-up events with modifier state. |
| `tpl_mouse_input` | Mouse move, button, and wheel events with screen position. |
| `tpl_gamepad_input` | Button and axis events from a connected gamepad. |
| `tpl_touch_input` | Multi-touch begin / move / end events with touch id. |
| `tpl_window_event` | Window resize, focus, and lifecycle events. |

The `browser-events` feature additionally enables these actors in a browser runtime.

## Typical wiring

```text
tpl_window_event ──► tpl_layout_sync ──► tpl_scene_graph (AssetDB write) ──► tpl_scene_render
tpl_mouse_input   ──► tpl_hit_test    ──► tpl_fsm                          ──► tpl_signal
```

See the `button_click` example for an end-to-end demo.

## Complete per-template catalog

See **[standard-library.md § Input Events](./standard-library.md)**.

## Related

- [Browser actors](./browser-actors.md) — headless browser automation behind the `browser` feature.
