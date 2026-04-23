"""Stream producer → consumer: send three byte chunks through a
Message.StreamHandle and drain them on the other side."""

from offbit_reflow import Stream


def main() -> None:
    s = Stream.create(
        buffer_size=16,
        origin_actor="producer",
        origin_port="out",
        content_type="application/octet-stream",
    )
    for buf in (b"alpha", b"bravo", b"charlie"):
        s.send_bytes(buf)
    s.end()

    msg = s.into_message()
    print("produced:", msg.kind)

    rx = msg.take_stream()
    collected = []
    while True:
        f = rx.recv(500)
        kind = f["kind"]
        if kind == "data":
            collected.append(bytes(f["data"]))
        elif kind in ("end", "closed"):
            break
        elif kind == "error":
            raise RuntimeError(f"stream error: {f.get('error')}")
        elif kind == "timeout":
            break
    print("collected:", collected)


if __name__ == "__main__":
    main()
