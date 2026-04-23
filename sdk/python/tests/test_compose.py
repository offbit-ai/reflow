from offbit_reflow import compose_graphs


def _export(name: str, procs: dict) -> dict:
    return {
        "caseSensitive": False,
        "processes": procs,
        "connections": [],
        "inports": {},
        "outports": {},
        "properties": {"name": name},
        "groups": [],
    }


def test_compose_merges_two_graphs():
    left = _export("left", {"a": {"id": "a", "component": "tpl_passthrough", "metadata": None}})
    right = _export("right", {"b": {"id": "b", "component": "tpl_passthrough", "metadata": None}})
    composed = compose_graphs({
        "graphs": [left, right],
        "connections": [],
        "shared_resources": [],
        "properties": {"name": "composed"},
        "case_sensitive": False,
    })
    procs = composed["processes"]
    assert any(k.endswith("/a") or k == "a" for k in procs)
    assert any(k.endswith("/b") or k == "b" for k in procs)


def test_compose_wires_cross_graph_connection():
    gsrc = _export("gsrc", {"src": {"id": "src", "component": "tpl_passthrough", "metadata": None}})
    gsink = _export("gsink", {"sink": {"id": "sink", "component": "tpl_passthrough", "metadata": None}})
    composed = compose_graphs({
        "graphs": [gsrc, gsink],
        "connections": [
            {"from": {"process": "gsrc/src", "port": "out"},
             "to":   {"process": "gsink/sink", "port": "in"}},
        ],
        "properties": {"name": "pipeline"},
    })
    assert composed["connections"], f"no connections in composed: {composed}"
