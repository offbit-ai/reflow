# Orchestrating multiple LLM agents

Tutorials 01 and 02 ran in a browser tab. This one moves to Python and
uses Reflow as a multi-agent orchestrator: three specialist research
agents run in parallel against a local Ollama model, a synthesizer
combines their findings, a logger streams role-tagged tokens to
stdout as they arrive, and a sink hands the final answer back to the
calling Python script.

```mermaid
flowchart LR
    topic([topic])
    topic --> factual[factual_researcher]
    topic --> stat[statistician]
    topic --> quote[quoter]
    factual --|chunk| logger[logger]
    stat    --|chunk| logger
    quote   --|chunk| logger
    factual --|finding| synth[synthesizer]
    stat    --|finding| synth
    quote   --|finding| synth
    synth   --|chunk|  logger
    synth   --|answer| sink[sink]
```

Two properties of the graph matter. The three specialists run
concurrently because no edge connects them — total latency is
`max(t_specialist) + t_synth`. Every agent emits per-token packets via
`ctx.send` while it streams, so the user sees progress live instead of
waiting for the synthesizer to finish.

## Prerequisites

```sh
ollama pull qwen2.5:3b
python -m venv .venv && source .venv/bin/activate
pip install offbit-reflow openai
```

The agents call the OpenAI-compatible chat-completions endpoint Ollama
serves at `http://localhost:11434/v1`. Set `OPENAI_BASE_URL` and
`OPENAI_API_KEY` to point at OpenAI / Groq / OpenRouter if you want a
hosted model — the code is identical.

## A specialist

Each specialist subclasses `Actor`, declares one inport (`topic`), two
outports (`chunk` for live tokens, `finding` for the full text), and
calls the LLM with `stream=True`.

```python
from openai import OpenAI
from offbit_reflow import Actor, Message

client = OpenAI(base_url="http://localhost:11434/v1", api_key="ollama")

def stream_chat(system: str, user: str):
    resp = client.chat.completions.create(
        model="qwen2.5:3b",
        messages=[{"role": "system", "content": system},
                  {"role": "user",   "content": user}],
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
    inports    = ["topic"]
    outports   = ["chunk", "finding"]
    role       = ""
    role_label = ""

    def run(self, ctx):
        topic = ctx.inputs["topic"]["data"]
        full, first = [], True
        for delta in stream_chat(self.role, f"Topic: {topic}"):
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
    role = "You are a factual researcher. Give 3–5 bullets."


class Statistician(Specialist):
    component  = "statistician"
    role_label = "stats"
    role = "You are a statistician. Give 3–5 bullets of relevant numbers."


class Quoter(Specialist):
    component  = "quoter"
    role_label = "quotes"
    role = "You are a quote librarian. Provide 2–3 short, attributable quotes."
```

`ctx.send(messages)` flushes a packet to the outport channel
immediately — the consumer fires before the agent's `run` returns.
`ctx.done(outputs)` resolves the tick. Use `ctx.send` for streaming
tokens; use `ctx.done` for the final value of the tick.

## The synthesizer

Set `await_all_inports = True`. The runtime accumulates packets across
declared inports and calls `run` once every inport has at least one
packet.

```python
class Synthesizer(Actor):
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
            "Combine these three perspectives into a 2–3 paragraph answer.\n\n"
            f"## Facts\n{facts}\n\n"
            f"## Stats\n{stats}\n\n"
            f"## Quotes\n{quotes}\n"
        )
        full, first = [], True
        for delta in stream_chat(
            "You are a senior writer. Compose a tight, sourced answer.",
            prompt,
        ):
            full.append(delta)
            ctx.send({
                "chunk": Message.object({
                    "role": self.role_label, "text": delta, "first": first,
                }),
            })
            first = False
        ctx.done({"answer": Message.string("".join(full))})
```

`await_all_inports` counts ports, not packets. A port is "ready" once
it has received any packet — from a connection or from
`add_initial`. This actor would not fire if any of `facts`, `stats`,
or `quotes` were missing data.

The synthesizer also streams its tokens on `chunk`, using the same
shape the specialists emit. The same logger handles them all.

## The logger

A fan-in node that prints role-tagged tokens to stdout. One inport
(`chunk`) bound to four sources.

```python
import sys

class Logger(Actor):
    component = "logger"
    inports   = ["chunk"]
    outports  = []

    def __init__(self):
        super().__init__()
        self._current_role = None

    def run(self, ctx):
        c = ctx.inputs["chunk"]["data"]
        role, text = c["role"], c["text"]
        if role != self._current_role:
            if self._current_role is not None:
                sys.stdout.write("\n\n")
            sys.stdout.write(f"[{role}] ")
            self._current_role = role
        sys.stdout.write(text)
        sys.stdout.flush()
        ctx.done()
```

The runtime fires `run` once per packet, so prints interleave across
the three parallel specialists. `[answer]` chunks follow once the
synthesizer kicks in.

## Bridging back to plain Python

Reflow runs the graph on its own scheduler; the calling Python script
is sync code that wants the answer back. `Sink` puts the final
answer on a `queue.Queue`; the script blocks on `queue.get()`.

```python
import queue

class Sink(Actor):
    component = "sink"
    inports   = ["answer"]
    outports  = []

    def __init__(self, q):
        super().__init__()
        self._q = q

    def run(self, ctx):
        self._q.put(ctx.inputs["answer"]["data"])
        ctx.done()
```

This is the standard pattern when you want to call a Reflow flow as
if it were a function.

## Wiring and running

```python
from offbit_reflow import Network

def run(topic: str) -> str:
    out = queue.Queue()

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

    net.add_connection("factual",      "finding", "synth", "facts")
    net.add_connection("statistician", "finding", "synth", "stats")
    net.add_connection("quoter",       "finding", "synth", "quotes")
    for src in ("factual", "statistician", "quoter", "synth"):
        net.add_connection(src, "chunk", "logger", "chunk")
    net.add_connection("synth", "answer", "sink", "answer")

    topic_msg = {"type": "String", "data": topic}
    net.add_initial("factual",      "topic", topic_msg)
    net.add_initial("statistician", "topic", topic_msg)
    net.add_initial("quoter",       "topic", topic_msg)

    net.start()
    try:
        return out.get(timeout=300)
    finally:
        net.shutdown()


if __name__ == "__main__":
    print(run("the history of zero"))
```

`add_initial` drops a packet directly on an actor's inport. The three
calls kick the specialists; from there the graph runs on its own
until the sink puts the synthesizer's answer on the queue.

## Notes on the design

- Adding an agent: write a class, register it, add an `add_node` and
  one or two `add_connection` calls. No other code changes.
- Mixed graphs: actors that hit the LLM and actors that don't (HTTP
  fetches, file reads, tool calls) compose the same way. The
  synthesizer doesn't know or care which kind sent its inputs.
- Cross-language: the same graph topology runs from any Reflow SDK.
  Move an agent to Go or the JVM by re-registering the template
  there.
- Authoring: the graph is JSON. You can build it programmatically
  (above) or in [Zeal](https://github.com/offbit-ai/zeal) and load
  it from disk.

## What is next

The next post takes the same graph shape into a long-running service:
a Go gRPC backend with a per-request network and streamed responses.
