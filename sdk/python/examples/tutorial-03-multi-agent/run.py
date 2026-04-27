"""Wire the agents into a Reflow graph and stream a research answer.

    PYTHONPATH=path/to/sdk/python  python run.py "history of zero"

The graph:

    topic ──┬─► factual_researcher ──┐
            │                          │
            ├─► statistician ──────── ┼─► synthesizer ──► sink
            │                          │
            └─► quoter ────────────── ┘

Every agent (the three specialists and the synthesizer) is a
streaming actor: as the model produces tokens, the agent emits one
Reflow packet per token on its `chunk` outport. A single `Logger`
actor fans in chunks from all four sources and prints them live, so
the user sees progress as it happens instead of waiting for the
graph to finish. The full text of each agent's output flows on the
`finding` / `answer` outport when streaming completes — that's what
the synthesizer waits on, and what the sink hands back to the
calling Python script.

The three specialists run concurrently — Reflow's runtime sees no
dependency between them and dispatches all three at once. Wall-clock
latency is `max(t_specialist) + t_synth`, not `sum(...)`.
"""

from __future__ import annotations

import argparse
import queue
import time

from offbit_reflow import Network

from agents import (
    FactualResearcher,
    Logger,
    Quoter,
    Sink,
    Statistician,
    Synthesizer,
)


def run(topic: str) -> tuple[str, float]:
    out: queue.Queue[str] = queue.Queue()

    net = Network()
    net.register_actor("tpl_factual",      FactualResearcher())
    net.register_actor("tpl_statistician", Statistician())
    net.register_actor("tpl_quoter",       Quoter())
    net.register_actor("tpl_synthesizer",  Synthesizer())
    net.register_actor("tpl_logger",       Logger())
    net.register_actor("tpl_sink",         Sink(out))

    for name, tpl in [
        ("factual",      "tpl_factual"),
        ("statistician", "tpl_statistician"),
        ("quoter",       "tpl_quoter"),
        ("synth",        "tpl_synthesizer"),
        ("logger",       "tpl_logger"),
        ("sink",         "tpl_sink"),
    ]:
        net.add_node(name, tpl)

    # Specialist findings → synthesizer
    net.add_connection("factual",      "finding", "synth", "facts")
    net.add_connection("statistician", "finding", "synth", "stats")
    net.add_connection("quoter",       "finding", "synth", "quotes")

    # Live chunks from every LLM-driven actor → logger
    for src in ("factual", "statistician", "quoter", "synth"):
        net.add_connection(src, "chunk", "logger", "chunk")

    # Synthesizer's full answer → sink
    net.add_connection("synth", "answer", "sink", "answer")

    topic_msg = {"type": "String", "data": topic}
    net.add_initial("factual",      "topic", topic_msg)
    net.add_initial("statistician", "topic", topic_msg)
    net.add_initial("quoter",       "topic", topic_msg)

    t0 = time.time()
    net.start()
    try:
        answer = out.get(timeout=300)
    finally:
        net.shutdown()
    return answer, time.time() - t0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("topic", nargs="+", help="topic to research")
    args = ap.parse_args()

    topic = " ".join(args.topic)
    print(f"Researching: {topic}")
    answer, elapsed = run(topic)
    print(f"\n\n--- final ({elapsed:.1f}s) ---\n")
    print(answer)
    print()


if __name__ == "__main__":
    main()
