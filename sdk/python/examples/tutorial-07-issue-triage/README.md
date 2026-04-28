# Tutorial 07 — Issue triage workflow (Python)

Runnable code for [Real-world Reflow 07: Composing a workflow from
the catalog](../../../../docs/tutorials/real-world/07-issue-triage.md).

A workflow that **reads issues, decides, and acts**. Almost every
node is a catalog template instantiated by id —
`api_github_list_issues`, `tpl_loop`, `tpl_switch`,
`api_slack_send_message`. The custom Python code is three small
actors: extract the body array, compute a routing key, and a
console fallback for the action sinks.

```
                   ┌─► [high_prio]   → ConsoleSink / api_slack_send_message
api_github_list_issues
       OR          ─► extract → tpl_loop → triage → tpl_switch ─┼─► [needs_owner] → ConsoleSink (would-comment)
   tpl_data_emit                                                │
                                                                ├─► [tracked]    → JsonlAppender → out/tracked.jsonl
                                                                └─► [default]    → ConsoleSink (dropped)
```

## Run (fixture mode — no credentials)

```sh
pip install 'offbit-reflow>=0.2.8'

# Get the api_services pack (one-time, per-platform):
PACK=https://github.com/offbit-ai/reflow/releases/download/pack-v0.2.4
TRIPLE=$(uname -m)-apple-darwin   # or x86_64-unknown-linux-gnu, etc.
curl -LO "$PACK/reflow.pack.api_services-0.2.0-$TRIPLE.rflpack"
mv reflow.pack.api_services-0.2.0-*.rflpack reflow.pack.api_services.rflpack

python3 pipeline.py
```

Output:

```
[would-slack] #412 Crashes on startup with Python 3.13   ...
[needs-owner] #388 Document new ctx.send mid-tick API    ...
[would-slack] #350 Memory leak when streaming huge files ...

archive: out/tracked.jsonl (1 rows)
```

The pipeline reads four fixture issues from `fixtures/issues.json`,
routes each through the same graph that the real-API mode uses, and
prints what *would* have been Slack-posted. Useful for testing the
graph wiring and routing logic without burning API credits.

## Run (real APIs)

```sh
export REFLOW_TUT07_LIVE=1
export GITHUB_API_KEY=ghp_…              # PAT with repo scope
export SLACK_API_KEY=xoxb-…              # optional; flips the high-prio sink to real Slack
export SLACK_CHANNEL=#ops-triage         # optional, defaults to #ops-triage

python3 pipeline.py
```

The graph is identical — the only difference is one `template_actor`
swap (`tpl_data_emit` → `api_github_list_issues`) and one optional
swap (`ConsoleSink` → `api_slack_send_message`).

## Why this earns its weight

- **The catalog is the workflow runtime.** Of the 8 nodes in the
  graph, only 4 (extract, triage, two console sinks, plus the JSONL
  appender) are custom. Source, iteration, routing, and the live API
  sinks are all catalog templates instantiated via
  `template_actor(id)`.
- **Routing is configuration, not code.** `tpl_switch` reads the
  `branch` field from the merged record and drops the packet on the
  matching outport — `case1`, `case2`, `case3`, or `default`. Want
  to add a fourth branch? Add `case4_value` to the config and one
  `add_connection`.
- **Mode swap is N=2 lines.** Every other node — including the
  custom decision actor — is identical between fixture and real-API
  mode.
