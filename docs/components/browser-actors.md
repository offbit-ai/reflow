# Browser Actors

Actors behind the `browser` and `browser-events` features of `reflow_components` / `reflow_rt`.

```toml
reflow_rt = { version = "0.1", features = ["browser"] }          # headless browser automation
reflow_rt = { version = "0.1", features = ["browser-events"] }   # browser runtime input events
```

## What's included

| Feature | Template | Purpose |
|---------|----------|---------|
| `browser` | `tpl_browser_screencast` | Drives a headless Chromium instance via `chromiumoxide`; emits frames as a Reflow stream. |
| `browser-events` | the `window-events` input actors | When compiled for a browser runtime, `tpl_keyboard_input`, `tpl_mouse_input`, `tpl_touch_input`, and `tpl_window_event` dispatch browser DOM events. |

## Requirements

- `browser` needs a reachable Chromium / Chrome installation, or a remote DevTools endpoint.
- `browser-events` is consumed by the Wasm network runtime (`reflow_network`'s `wasm` feature).

## Related

- [Input / window event actors](./input-actors.md) — same templates, native runtime side.
- See the `browser_screencast` example for `tpl_browser_screencast` end-to-end.
