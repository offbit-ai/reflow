"""Two-node pipeline: a Doubler actor emits 2×N into a Log actor.

Run after installing the SDK (``maturin develop`` inside sdk/python or
``pip install offbit-reflow``):

    python examples/double_and_log.py
"""

from __future__ import annotations

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


class Log(Actor):
    component = "log"
    inports = ["in"]
    outports = []

    def __init__(self, label: str = "got"):
        super().__init__()
        self.label = label

    def run(self, ctx):
        print(f"{self.label}:", ctx.inputs["in"])
        ctx.done()


def main() -> None:
    net = Network()
    net.register_actor("tpl_doubler", Doubler())
    net.register_actor("tpl_log", Log("doubled"))

    net.add_node("a", "tpl_doubler")
    net.add_node("b", "tpl_log")
    net.add_connection("a", "out", "b", "in")
    net.add_initial("a", "in", {"type": "Integer", "data": 21})

    net.start()
    time.sleep(0.3)
    net.shutdown()


if __name__ == "__main__":
    main()
