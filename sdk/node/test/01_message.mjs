import { test } from "node:test";
import assert from "node:assert/strict";
import { Message } from "../reflow.mjs";

test("Message constructors produce the expected kind", () => {
  assert.equal(Message.flow().kind, "Flow");
  assert.equal(Message.boolean(true).kind, "Boolean");
  assert.equal(Message.integer(7).kind, "Integer");
  assert.equal(Message.float(3.14).kind, "Float");
  assert.equal(Message.string("hello").kind, "String");
  assert.equal(Message.bytes(Buffer.from([1, 2, 3])).kind, "Bytes");
  assert.equal(Message.error("nope").kind, "Error");
  assert.equal(Message.fromJson({ type: "Flow" }).kind, "Flow");
  assert.equal(Message.object({ a: 1 }).kind, "Object");
  assert.equal(Message.array([1, 2, 3]).kind, "Array");
});

test("Message accessors return values for the matching variant only", () => {
  const m = Message.integer(42);
  assert.equal(m.asInteger(), 42);
  assert.equal(m.asBoolean(), null);
  assert.equal(m.asFloat(), null);
  assert.equal(m.asString(), null);

  const s = Message.string("abc");
  assert.equal(s.asString(), "abc");

  const b = Message.bytes(Buffer.from("abc"));
  const back = b.asBytes();
  assert.ok(Buffer.isBuffer(back));
  assert.equal(back.toString(), "abc");
});

test("asJson round-trips through fromJson", () => {
  const m = Message.integer(11);
  const j = m.asJson();
  const back = Message.fromJson(j);
  assert.equal(back.asInteger(), 11);
});
