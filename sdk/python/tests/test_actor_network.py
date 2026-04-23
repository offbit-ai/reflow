import threading
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


def test_network_pipeline_end_to_end():
    bucket = []
    net = Network()
    net.register_actor("tpl_doubler", Doubler())
    net.register_actor("tpl_collect", Collect(bucket))

    net.add_node("d", "tpl_doubler")
    net.add_node("c", "tpl_collect")
    net.add_connection("d", "out", "c", "in")
    net.add_initial("d", "in", {"type": "Integer", "data": 21})

    net.start()

    deadline = time.time() + 2.0
    while not bucket and time.time() < deadline:
        time.sleep(0.02)

    net.shutdown()

    assert len(bucket) == 1, f"bucket={bucket}"
    assert bucket[0]["type"] == "Integer"
    assert bucket[0]["data"] == 42


def test_event_stream_yields_network_started():
    class Nop(Actor):
        component = "nop"
        inports = ["in"]
        outports = []

        def run(self, ctx):
            ctx.done()

    net = Network()
    net.register_actor("tpl_nop", Nop())
    net.add_node("n", "tpl_nop")

    events = net.events()
    net.start()

    seen = set()
    deadline = time.time() + 1.5
    while time.time() < deadline and not ({"NetworkStarted", "ActorStarted"} <= seen):
        evt = events.recv(timeout_ms=150)
        if evt and "_type" in evt:
            seen.add(evt["_type"])

    net.shutdown()

    assert "NetworkStarted" in seen
    assert "ActorStarted" in seen


def test_ctx_emit_multi_port():
    """emit() accumulates across calls before done()."""

    class Splitter(Actor):
        component = "split"
        inports = ["in"]
        outports = ["a", "b"]

        def run(self, ctx):
            ctx.emit("a", Message.integer(1))
            ctx.emit("b", Message.integer(2))
            ctx.done()

    got_a, got_b = [], []

    class Sink:
        def __init__(self, bucket, port):
            self._bucket = bucket
            self._port = port

    class SinkA(Actor):
        component = "sink_a"
        inports = ["in"]
        def run(self, ctx):
            got_a.append(ctx.inputs["in"])
            ctx.done()

    class SinkB(Actor):
        component = "sink_b"
        inports = ["in"]
        def run(self, ctx):
            got_b.append(ctx.inputs["in"])
            ctx.done()

    net = Network()
    net.register_actor("tpl_split", Splitter())
    net.register_actor("tpl_sink_a", SinkA())
    net.register_actor("tpl_sink_b", SinkB())
    net.add_node("s", "tpl_split")
    net.add_node("a", "tpl_sink_a")
    net.add_node("b", "tpl_sink_b")
    net.add_connection("s", "a", "a", "in")
    net.add_connection("s", "b", "b", "in")
    net.add_initial("s", "in", {"type": "Flow"})
    net.start()
    deadline = time.time() + 2.0
    while (not got_a or not got_b) and time.time() < deadline:
        time.sleep(0.02)
    net.shutdown()

    assert got_a and got_a[0]["data"] == 1
    assert got_b and got_b[0]["data"] == 2
