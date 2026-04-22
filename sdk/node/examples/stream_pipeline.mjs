// Stream producer → consumer demo: send three byte chunks through a
// Message.StreamHandle and drain them on the other side.
import { Stream } from "../reflow.mjs";

const s = Stream.create({
  bufferSize: 16,
  originActor: "producer",
  originPort: "out",
  contentType: "application/octet-stream",
});

for (const buf of [Buffer.from("alpha"), Buffer.from("bravo"), Buffer.from("charlie")]) {
  s.sendBytes(buf);
}
s.end();

const msg = s.intoMessage();
console.log("produced:", msg.kind);

const rx = msg.takeStream();
const collected = [];
let done = false;
while (!done) {
  const f = await rx.recv(500);
  switch (f.kind) {
    case "begin":
      console.log("begin", f.contentType);
      break;
    case "data":
      collected.push(Buffer.from(f.data).toString());
      break;
    case "end":
    case "closed":
      done = true;
      break;
    case "error":
      console.error("stream error:", f.error);
      done = true;
      break;
    case "timeout":
      done = true;
      break;
  }
}
console.log("collected:", collected);
