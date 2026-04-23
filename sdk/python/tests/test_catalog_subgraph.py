import pytest

from offbit_reflow import Actor, Network, SubgraphBuilder, template_actor, template_list


def test_template_list_includes_known_id():
    ids = template_list()
    assert len(ids) > 100
    assert "tpl_passthrough" in ids


def test_template_actor_resolves_bundled():
    a = template_actor("tpl_passthrough")
    assert a is not None


def test_template_actor_raises_on_unknown():
    with pytest.raises(Exception):
        template_actor("tpl_does_not_exist")


def test_subgraph_builder_with_registered_actor():
    class Tap(Actor):
        component = "my_tap"
        inports = ["in"]
        outports = ["out"]
        def run(self, ctx):
            ctx.emit("out", ctx.inputs.get("in", {"type": "Flow"}))
            ctx.done()

    export = {
        "caseSensitive": False,
        "processes": {
            "t": {"id": "t", "component": "my_tap", "metadata": None}
        },
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {"name": "sg"},
        "groups": [],
    }
    builder = SubgraphBuilder(export)
    builder.register_actor("my_tap", Tap())
    sg = builder.build()

    net = Network()
    net.register_actor("tpl_sub", sg)
