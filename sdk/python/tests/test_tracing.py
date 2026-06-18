"""First-class tracing in the Python SDK.

Tracing is enabled via the network config; trace events are consumed locally
through ``Network.traces()`` with no collector required. Verifies correlation
(events arrive) and fidelity (data-flow snapshots carry a content checksum).
"""

import time

from offbit_reflow import Actor, Message, Network


class Doubler(Actor):
    component = "doubler"
    inports = ["in"]
    outports = ["out"]

    def run(self, ctx):
        n = ctx.inputs["in"]["data"]
        ctx.emit("out", Message.integer(n * 2))
        ctx.done()


class Collect(Actor):
    component = "collect"
    inports = ["in"]
    outports = []

    def __init__(self, bucket):
        super().__init__()
        self.bucket = bucket

    def run(self, ctx):
        self.bucket.append(ctx.inputs["in"])
        ctx.done()


def _event_type_names(evt):
    """TraceEventType is a string for unit variants ("ActorCreated") or a
    single-key dict for data-carrying variants ({"DataFlow": {...}})."""
    et = evt.get("event_type")
    if isinstance(et, str):
        return {et}
    if isinstance(et, dict):
        return set(et.keys())
    return set()


def test_trace_stream_yields_correlated_events_with_checksums():
    bucket = []
    # Tracing enabled. The collector URL need not be reachable: events still
    # flow to the local tap. (A real deployment points server_url at a server.)
    net = Network(
        {"tracing": {"server_url": "ws://127.0.0.1:8080", "enabled": True}}
    )
    net.register_actor("tpl_doubler", Doubler())
    net.register_actor("tpl_collect", Collect(bucket))
    net.add_node("d", "tpl_doubler")
    net.add_node("c", "tpl_collect")
    net.add_connection("d", "out", "c", "in")
    net.add_initial("d", "in", {"type": "Integer", "data": 21})

    traces = net.traces()  # subscribe before start
    net.start()

    seen_types = set()
    saw_checksum = False
    deadline = time.time() + 3.0
    while time.time() < deadline:
        evt = traces.recv(timeout_ms=200)
        if evt is None:
            continue
        seen_types |= _event_type_names(evt)
        msg = (evt.get("data") or {}).get("message")
        if msg and isinstance(msg.get("checksum"), str) and msg["checksum"].startswith("sha256:"):
            saw_checksum = True
        if "ActorCreated" in seen_types and saw_checksum:
            break

    net.shutdown()

    assert "ActorCreated" in seen_types, f"seen event types: {seen_types}"
    assert saw_checksum, "expected a data-flow snapshot carrying a sha256 checksum"
