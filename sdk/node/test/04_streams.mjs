import { test } from "node:test";
import assert from "node:assert/strict";
import { Stream, Message } from "../reflow.mjs";

test("Stream producer → consumer round-trip", async () => {
  const s = Stream.create({ bufferSize: 16, originActor: "a", originPort: "out" });
  s.sendBytes(Buffer.from([1, 2, 3]));
  s.sendBytes(Buffer.from([9, 8, 7, 6]));
  s.end();

  const msg = s.intoMessage();
  assert.equal(msg.kind, "StreamHandle");
  const rx = msg.takeStream();

  const chunks = [];
  let sawEnd = false;
  for (let i = 0; i < 20 && !sawEnd; i++) {
    const f = await rx.recv(250);
    if (f.kind === "data") chunks.push(Buffer.from(f.data));
    else if (f.kind === "end" || f.kind === "closed") sawEnd = true;
    else if (f.kind === "timeout") continue;
    else if (f.kind === "error") throw new Error(`stream error: ${f.error}`);
  }

  assert.ok(sawEnd, "never saw end frame");
  assert.equal(chunks.length, 2);
  assert.deepEqual([...chunks[0]], [1, 2, 3]);
  assert.deepEqual([...chunks[1]], [9, 8, 7, 6]);
});

test("Stream error frame surfaces the message", async () => {
  const s = Stream.create();
  s.error("bad data");
  const msg = s.intoMessage();
  const rx = msg.takeStream();

  let seen = null;
  for (let i = 0; i < 5; i++) {
    const f = await rx.recv(250);
    if (f.kind === "error") { seen = f.error; break; }
    if (f.kind === "closed" || f.kind === "timeout") break;
  }
  assert.equal(seen, "bad data");
});
