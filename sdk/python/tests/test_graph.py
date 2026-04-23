import json

from offbit_reflow import Graph


def test_graph_authoring_round_trips():
    g = Graph("demo", case_sensitive=False)
    g.add_node("a", "tpl_x")
    g.add_node("b", "tpl_y", {"role": "sink"})
    g.add_connection("a", "out", "b", "in")
    g.add_initial("a", "in", {"type": "Flow"})

    data = g.to_json()
    assert "a" in data["processes"]
    assert data["processes"]["a"]["component"] == "tpl_x"
    assert data["processes"]["b"]["metadata"]["role"] == "sink"

    regular = [c for c in data["connections"] if c.get("data") is None]
    initials = [c for c in data["connections"] if c.get("data") is not None]
    assert len(regular) == 1
    assert len(initials) == 1


def test_graph_from_json_round_trips():
    src = {
        "caseSensitive": False,
        "processes": {"n": {"id": "n", "component": "tpl_passthrough", "metadata": None}},
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {"name": "demo"},
        "groups": [],
    }
    g = Graph.from_json(src)
    data = g.to_json()
    assert data["processes"]["n"]["component"] == "tpl_passthrough"
    # Confirm roundtrip is stable as serialized JSON.
    json.dumps(data)


def test_remove_node():
    g = Graph()
    g.add_node("a", "tpl_x")
    g.remove_node("a")
    assert g.to_json()["processes"] == {}
