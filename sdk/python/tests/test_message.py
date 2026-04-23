from offbit_reflow import Message


def test_constructors_yield_expected_kind():
    assert Message.flow().kind == "Flow"
    assert Message.boolean(True).kind == "Boolean"
    assert Message.integer(7).kind == "Integer"
    assert Message.float(3.14).kind == "Float"
    assert Message.string("hi").kind == "String"
    assert Message.bytes(b"abc").kind == "Bytes"
    assert Message.error("bad").kind == "Error"
    assert Message.object({"a": 1}).kind == "Object"
    assert Message.array([1, 2, 3]).kind == "Array"
    assert Message.from_json({"type": "Flow"}).kind == "Flow"


def test_accessors_match_variant():
    m = Message.integer(42)
    assert m.as_integer() == 42
    assert m.as_boolean() is None
    assert m.as_float() is None

    assert Message.string("abc").as_string() == "abc"
    assert Message.bytes(b"abc").as_bytes() == b"abc"


def test_as_json_round_trip():
    m = Message.integer(11)
    back = Message.from_json(m.as_json())
    assert back.as_integer() == 11
