"""Issue triage workflow — read issues, decide via rules engine,
take action.

The routing decision is **JSON config on `tpl_rules_engine` actors**,
not custom Python. Each rule has its own actor; the chain forms a
tree where `matched` goes to the action sink and `unmatched`
falls through to the next rule.

  IssueNormalize → high_prio_rule
                     ├─ matched ──► high-priority sink
                     └─ unmatched → owner_rule
                                      ├─ matched ──► needs-owner sink
                                      └─ unmatched → tracked archive

Custom Python (in actors.py): `ExtractIssues`, `IssueNormalize`,
`JsonlAppender`, `ConsoleSink`. Routing is config.

Run modes:

  HEURISTIC (default, no credentials)
    Source → tpl_data_emit (fixtures/issues.json)
    Action sinks → ConsoleSink prints what would have been sent.

  REAL APIs (REFLOW_TUT07_LIVE=1, GITHUB_API_KEY, optional SLACK_API_KEY)
    Source → api_github_list_issues
    Slack action → api_slack_send_message
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import offbit_reflow as reflow
from offbit_reflow import Network

from actors import ConsoleSink, ExtractIssues, IssueNormalize, JsonlAppender

PACK_PATH = os.environ.get(
    "REFLOW_PACK_API_SERVICES",
    str(Path(__file__).parent / "reflow.pack.api_services.rflpack"),
)
FIXTURE = Path(__file__).parent / "fixtures" / "issues.json"
ARCHIVE = Path(__file__).parent / "out" / "tracked.jsonl"

LIVE = os.environ.get("REFLOW_TUT07_LIVE") == "1"
HAVE_SLACK = bool(os.environ.get("SLACK_API_KEY"))


# ── Rule definitions ─────────────────────────────────────────────
# Same shape `tpl_rules_engine` accepts in its `rules` config field.
# Each rule's `actions.setProperty` enriches the matched record so
# downstream sinks see the routing tag.

HIGH_PRIO_RULE = {
    "rules": {
        "type": "IF",
        "groups": [
            {
                # Any of: priority:high label, or bug label.
                "connector": "OR",
                "rules": [
                    {"field": "labels", "operator": "contains", "value": "priority:high"},
                    {"field": "labels", "operator": "contains", "value": "bug"},
                ],
            },
        ],
        "actions": {
            "setProperty": [{"key": "branch", "value": "high_prio"}],
        },
    },
}

NEEDS_OWNER_RULE = {
    "rules": {
        "type": "IF",  # all groups must match
        "groups": [
            {
                "connector": "AND",
                "rules": [
                    {"field": "has_assignee", "operator": "is",          "value": False},
                    {"field": "age_days",     "operator": "greater_equal", "value": 3},
                ],
            },
        ],
        "actions": {
            "setProperty": [{"key": "branch", "value": "needs_owner"}],
        },
    },
}


def main() -> int:
    reflow.load_pack(PACK_PATH)
    ARCHIVE.parent.mkdir(exist_ok=True)
    ARCHIVE.unlink(missing_ok=True)

    net = Network()

    # ── Source ────────────────────────────────────────────────────
    if LIVE:
        net.register_actor("tpl_source", reflow.template_actor("api_github_list_issues"))
        net.add_node("source", "tpl_source", config={"state": "open", "filter": "assigned"})
    else:
        with open(FIXTURE) as f:
            fixture_issues = json.load(f)
        net.register_actor("tpl_source", reflow.template_actor("tpl_data_emit"))
        net.add_node("source", "tpl_source", config={
            "data": {"status": 200, "headers": {}, "body": fixture_issues},
            "oneshot": True,
        })

    # ── Pipeline ──────────────────────────────────────────────────
    net.register_actor("tpl_extract", ExtractIssues()._build())
    net.add_node("extract", "tpl_extract")

    net.register_actor("tpl_loop", reflow.template_actor("tpl_loop"))
    net.add_node("each", "tpl_loop")

    net.register_actor("tpl_normalize", IssueNormalize()._build())
    net.add_node("normalize", "tpl_normalize")

    # ── Routing tree (rules-engine chain) ────────────────────────
    # Each rules engine emits matched (rule fired, branch property
    # added) or unmatched (untouched). matched lands at a sink;
    # unmatched cascades to the next rule.
    net.register_actor("tpl_rule_high",  reflow.template_actor("tpl_rules_engine"))
    net.add_node("rule_high",  "tpl_rule_high",  config=HIGH_PRIO_RULE)

    net.register_actor("tpl_rule_owner", reflow.template_actor("tpl_rules_engine"))
    net.add_node("rule_owner", "tpl_rule_owner", config=NEEDS_OWNER_RULE)

    # ── Sinks ─────────────────────────────────────────────────────
    if LIVE and HAVE_SLACK:
        net.register_actor("tpl_slack", reflow.template_actor("api_slack_send_message"))
        net.add_node("sink_high", "tpl_slack", config={
            "channel": os.environ.get("SLACK_CHANNEL", "#ops-triage"),
        })
    else:
        net.register_actor("tpl_sink_high", ConsoleSink("would-slack")._build())
        net.add_node("sink_high", "tpl_sink_high")

    net.register_actor("tpl_sink_owner", ConsoleSink("needs-owner")._build())
    net.add_node("sink_owner", "tpl_sink_owner")

    net.register_actor("tpl_archive", JsonlAppender(str(ARCHIVE))._build())
    net.add_node("archive", "tpl_archive")

    # ── Wiring ────────────────────────────────────────────────────
    if LIVE:
        net.add_initial("source", "filter", {"type": "String", "data": "assigned"})
    else:
        net.add_initial("source", "trigger", {"type": "Flow"})

    src_out = "response" if LIVE else "output"
    net.add_connection("source",    src_out,    "extract",    "response")
    net.add_connection("extract",   "issues",   "each",       "collection")
    net.add_connection("each",      "item",     "normalize",  "item")

    # rules engine chain
    net.add_connection("normalize", "issue",     "rule_high",  "data")
    net.add_connection("rule_high", "matched",   "sink_high",  "routed")
    net.add_connection("rule_high", "unmatched", "rule_owner", "data")
    net.add_connection("rule_owner","matched",   "sink_owner", "routed")
    net.add_connection("rule_owner","unmatched", "archive",    "data")

    net.start()
    import time
    time.sleep(1.0)
    net.shutdown()

    print()
    if ARCHIVE.exists():
        print(f"archive: {ARCHIVE} ({sum(1 for _ in open(ARCHIVE))} rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
