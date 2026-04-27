# Tutorial 02 — Live Wikipedia edits

Runnable code for [Real-world Reflow 02: Live edits over a
stream](../../../../docs/tutorials/real-world/02-live-edits.md).

## Run

```sh
npx serve .
```

Open the URL it prints. Edits scroll in as they happen on
en.wikipedia.

The HTML pulls `@offbit-ai/reflow` from `esm.sh` and connects to
Wikimedia's public EventSource feed. No local install or auth.
