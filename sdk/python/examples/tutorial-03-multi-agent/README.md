# Tutorial 03 — Multi-agent orchestration with Reflow

Runnable code for [Real-world Reflow 03: Orchestrating multiple LLM
agents](../../../../docs/tutorials/real-world/03-multi-agent.md).

Three specialist research agents run in parallel against an Ollama
model, a synthesizer combines their findings, and a sink hands the
final answer back to the calling Python script. Reflow's actor
model is doing the orchestration — no `asyncio.gather`, no manual
fan-out.

## Prerequisites

Install Ollama (https://ollama.com) and pull the model:

```sh
ollama pull qwen2.5:3b
```

Anything Ollama serves works. Heavier Qwen variants
(`qwen2.5:7b`, `qwen2.5-coder:7b`) give noticeably better
synthesis if you have the RAM.

## Run

```sh
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt

# Replace `path/to/sdk/python` with the path to this repo's Python SDK
# (or just `pip install offbit-reflow` from PyPI when the bump publishes).
PYTHONPATH=path/to/sdk/python python run.py "the history of zero"
```

The first call takes a few seconds while Ollama warms the model
into memory. Subsequent calls are faster.

## Swap the model / provider

The agents talk to an OpenAI-compatible endpoint. Override via env:

| Variable | Default | Purpose |
|---|---|---|
| `OPENAI_BASE_URL` | `http://localhost:11434/v1` | Ollama's compat endpoint. Point at OpenAI / Groq / OpenRouter to use a hosted model. |
| `OPENAI_API_KEY` | `ollama` | Ignored by Ollama; required by the OpenAI SDK. |
| `REFLOW_MODEL` | `qwen2.5:3b` | Any model your endpoint serves. |
