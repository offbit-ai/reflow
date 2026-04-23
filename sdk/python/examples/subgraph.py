"""Compose a subgraph from a Python-authored actor and register the
whole subgraph as a single template on the parent network."""

from offbit_reflow import Actor, Network, SubgraphBuilder


class Tap(Actor):
    component = "my_tap"
    inports = ["in"]
    outports = ["out"]

    def run(self, ctx):
        ctx.emit("out", ctx.inputs.get("in", {"type": "Flow"}))
        ctx.done()


def main() -> None:
    export = {
        "caseSensitive": False,
        "processes": {
            "t": {"id": "t", "component": "my_tap", "metadata": None},
        },
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {"name": "tap-subgraph"},
        "groups": [],
    }
    builder = SubgraphBuilder(export)
    builder.register_actor("my_tap", Tap())
    sg = builder.build()
    print("built subgraph actor")

    net = Network()
    net.register_actor("tpl_sub", sg)
    net.add_node("s", "tpl_sub")
    print("registered + added to network")


if __name__ == "__main__":
    main()
