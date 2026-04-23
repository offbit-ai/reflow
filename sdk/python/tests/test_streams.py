from offbit_reflow import Stream


def test_stream_producer_consumer_round_trip():
    s = Stream.create(buffer_size=16, origin_actor="a", origin_port="out")
    s.send_bytes(b"\x01\x02\x03")
    s.send_bytes(b"\x09\x08\x07\x06")
    s.end()

    msg = s.into_message()
    assert msg.kind == "StreamHandle"

    rx = msg.take_stream()
    collected = []
    saw_end = False
    for _ in range(20):
        f = rx.recv(250)
        if f["kind"] == "data":
            collected.append(f["data"])
        elif f["kind"] in ("end", "closed"):
            saw_end = True
            break
        elif f["kind"] == "timeout":
            continue
        elif f["kind"] == "error":
            raise AssertionError(f"stream error: {f.get('error')}")

    assert saw_end
    assert collected == [b"\x01\x02\x03", b"\x09\x08\x07\x06"]


def test_stream_error_frame_surfaces_message():
    s = Stream.create()
    s.error("bad data")
    msg = s.into_message()
    rx = msg.take_stream()

    seen = None
    for _ in range(5):
        f = rx.recv(250)
        if f["kind"] == "error":
            seen = f["error"]
            break
        if f["kind"] in ("closed", "timeout"):
            break
    assert seen == "bad data"
