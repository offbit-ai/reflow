"""Specialist research agents + a synthesizer + a live progress logger.

Each agent talks to a local Ollama model through the OpenAI-compatible
chat-completions endpoint, **with `stream=True`**. As tokens arrive,
the agent emits one Reflow packet per chunk on its `chunk` outport,
and a `Logger` actor fans those packets in and prints them live to
stdout. The full text of each agent's finding is sent on a separate
`finding` outport once streaming completes — that is what the
synthesizer waits on.

Reflow's actor model orchestrates the four LLM-driven actors (three
specialists + the synthesizer): the specialists run in parallel
because there is no data dependency between them, and the synthesizer
waits for all three findings via `await_all_inports = True` before it
fires. The Logger fires once per chunk, regardless of which agent it
came from.

To swap providers, set `OPENAI_BASE_URL` and `OPENAI_API_KEY` in env.
The default Ollama setup needs no key — Ollama ignores the field, but
the OpenAI SDK requires *some* string.
"""

from __future__ import annotations

import os
import queue
import sys
from typing import Iterator

from openai import OpenAI

from offbit_reflow import Actor, Message


_BASE_URL = os.environ.get("OPENAI_BASE_URL", "http://localhost:11434/v1")
_API_KEY  = os.environ.get("OPENAI_API_KEY",  "ollama")
_MODEL    = os.environ.get("REFLOW_MODEL",    "qwen2.5:3b")

_client: OpenAI | None = None


def _stream_chat(system: str, user: str) -> Iterator[str]:
    """Yield content deltas from the model as they arrive."""
    global _client
    if _client is None:
        _client = OpenAI(base_url=_BASE_URL, api_key=_API_KEY)
    resp = _client.chat.completions.create(
        model=_MODEL,
        messages=[
            {"role": "system", "content": system},
            {"role": "user",   "content": user},
        ],
        temperature=0.4,
        stream=True,
    )
    for chunk in resp:
        if not chunk.choices:
            continue
        delta = chunk.choices[0].delta.content or ""
        if delta:
            yield delta


class Specialist(Actor):
    """Base for the three parallel research agents.

    `role` is the system prompt; `role_label` is the short tag the
    Logger prints alongside live tokens. Each specialist emits one
    packet per token on `chunk` and the full text on `finding` when
    streaming is done.
    """

    inports    = ["topic"]
    outports   = ["chunk", "finding"]
    role       = ""
    role_label = ""

    def run(self, ctx):
        topic = ctx.inputs["topic"]["data"]
        full: list[str] = []
        first = True
        for delta in _stream_chat(self.role, f"Topic: {topic}"):
            full.append(delta)
            ctx.send({
                "chunk": Message.object({
                    "role":  self.role_label,
                    "text":  delta,
                    "first": first,
                }),
            })
            first = False
        ctx.done({"finding": Message.string("".join(full))})


class FactualResearcher(Specialist):
    component  = "factual_researcher"
    role_label = "facts"
    role = (
        "You are a factual researcher. Give 3–5 bullets covering the "
        "key facts about the topic. Be concise and concrete."
    )


class Statistician(Specialist):
    component  = "statistician"
    role_label = "stats"
    role = (
        "You are a statistician. Give 3–5 bullets of relevant numbers, "
        "trends, or measurements about the topic. Cite rough timeframes "
        "when applicable."
    )


class Quoter(Specialist):
    component  = "quoter"
    role_label = "quotes"
    role = (
        "You are a quote librarian. Provide 2–3 short, attributable "
        "quotes that illuminate the topic. Include the source where "
        "you can; otherwise mark it (apocryphal)."
    )


class Synthesizer(Actor):
    """Waits for all three specialists' findings, streams a combined
    answer, and emits the full text on `answer`."""

    component         = "synthesizer"
    inports           = ["facts", "stats", "quotes"]
    outports          = ["chunk", "answer"]
    await_all_inports = True
    role_label        = "answer"

    def run(self, ctx):
        facts  = ctx.inputs["facts"]["data"]
        stats  = ctx.inputs["stats"]["data"]
        quotes = ctx.inputs["quotes"]["data"]
        prompt = (
            "Combine these three perspectives into a 2–3 paragraph answer "
            "that flows naturally. Cite sources / quotes inline.\n\n"
            f"## Facts\n{facts}\n\n"
            f"## Stats\n{stats}\n\n"
            f"## Quotes\n{quotes}\n"
        )
        full: list[str] = []
        first = True
        for delta in _stream_chat(
            "You are a senior writer. Compose a tight, sourced answer "
            "from the supplied research. Do not invent facts.",
            prompt,
        ):
            full.append(delta)
            ctx.send({
                "chunk": Message.object({
                    "role":  self.role_label,
                    "text":  delta,
                    "first": first,
                }),
            })
            first = False
        ctx.done({"answer": Message.string("".join(full))})


class Logger(Actor):
    """Prints role-tagged chunks to stdout as they arrive. Fans in
    streams from every specialist plus the synthesizer.

    The runtime fires `run` once per packet, so the print order is
    "whichever agent's token arrived next" — interleaved across the
    three parallel specialists exactly as they stream from the model.
    """

    component = "logger"
    inports   = ["chunk"]
    outports  = []

    def __init__(self):
        super().__init__()
        self._current_role: str | None = None

    def run(self, ctx):
        c = ctx.inputs["chunk"]["data"]
        role = c["role"]
        text = c["text"]
        # When the role changes, break to a new line and print a tag.
        if role != self._current_role:
            if self._current_role is not None:
                sys.stdout.write("\n\n")
            sys.stdout.write(f"[{role}] ")
            self._current_role = role
        sys.stdout.write(text)
        sys.stdout.flush()
        ctx.done()


class Sink(Actor):
    """Pushes the final answer onto a Python queue so the outer
    script can wait on it."""

    component = "sink"
    inports   = ["answer"]
    outports  = []

    def __init__(self, q: queue.Queue):
        super().__init__()
        self._q = q

    def run(self, ctx):
        self._q.put(ctx.inputs["answer"]["data"])
        ctx.done()
