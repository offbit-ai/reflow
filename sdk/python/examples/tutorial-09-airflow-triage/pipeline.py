"""Reflow Network that the Airflow PythonOperator calls.

Same shape as tutorial 07: catalog actors handle the heavy lifting
(api_github_list_issues, tpl_loop, tpl_rules_engine,
api_slack_send_message), three custom actors fill the gaps. The
difference is *where* the network runs — inside one Airflow task
instance, with credentials pulled from Airflow Connections /
Variables and the result returned via XCom.
"""

from __future__ import annotations

import json
import os
import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import offbit_reflow as reflow
from offbit_reflow import Actor, Message, Network


# ── Custom actors ─────────────────────────────────────────────────


class ExtractIssues(Actor):
    component = "extract_issues"
    inports = ["response"]
    outports = ["issues"]

    def run(self, ctx) -> None:
        envelope = ctx.inputs["response"]["data"]
        if not isinstance(envelope, dict):
            ctx.fail(f"expected response object, got {type(envelope).__name__}")
            return
        status = envelope.get("status")
        body = envelope.get("body")
        if status and status >= 400:
            ctx.fail(f"GitHub API returned {status}: {str(body)[:200]}")
            return
        if not isinstance(body, list):
            ctx.fail(f"expected list of issues, got {type(body).__name__}")
            return
        ctx.done({"issues": Message.array(body)})


class IssueNormalize(Actor):
    component = "issue_normalize"
    inports = ["item"]
    outports = ["issue"]

    def run(self, ctx) -> None:
        wrapper = ctx.inputs["item"]["data"]
        issue = wrapper.get("value", wrapper) if isinstance(wrapper, dict) else wrapper

        labels = sorted({l.get("name", "") for l in (issue.get("labels") or [])})
        normalized = {
            "number":       issue.get("number"),
            "title":        issue.get("title"),
            "url":          issue.get("html_url"),
            "labels":       labels,
            "comments":     int(issue.get("comments") or 0),
            "has_assignee": issue.get("assignee") is not None,
            "assignee":     (issue.get("assignee") or {}).get("login"),
            "user":         (issue.get("user") or {}).get("login"),
            "age_days":     _age_days(issue.get("created_at")),
        }
        ctx.done({"issue": Message.object(normalized)})


class JsonlAppender(Actor):
    """Append-mode JSONL writer + completion counter. The counter
    lets the Airflow operator wait for the network to drain before
    returning."""

    component = "jsonl_appender"
    inports = ["data"]
    outports = []

    def __init__(self, path: str, counter: dict, cv: threading.Condition) -> None:
        self.path = path
        self.counter = counter
        self.cv = cv

    def run(self, ctx) -> None:
        row = ctx.inputs["data"]["data"]
        Path(self.path).parent.mkdir(parents=True, exist_ok=True)
        with open(self.path, "a") as f:
            f.write(json.dumps(row, separators=(",", ":")) + "\n")
        with self.cv:
            self.counter["written"] += 1
            self.cv.notify_all()
        ctx.done()


class SlackFormatter(Actor):
    """Builds a Slack-formatted text message from a matched issue
    object. `api_slack_send_message` expects `text` (string) on its
    inport, not the raw routed object — this is the small adapter
    that bridges the catalog actor's contract.
    """

    component = "slack_formatter"
    inports = ["routed"]
    outports = ["text"]

    def run(self, ctx) -> None:
        issue = ctx.inputs["routed"]["data"]
        text = (
            f":rotating_light: high-prio #{issue.get('number')} "
            f"*{issue.get('title')}*\n"
            f"{issue.get('url')} "
            f"(labels: {', '.join(issue.get('labels') or [])})"
        )
        ctx.done({"text": Message.string(text)})


# ── The function Airflow's PythonOperator calls ──────────────────


def run_triage(
    ds: str,
    *,
    pack_path: str,
    output_dir: str,
    slack_channel: str,
    timeout_seconds: float = 300.0,
) -> dict[str, Any]:
    """Run one day's triage. The body is identical to a stand-alone
    Reflow script — Airflow doesn't change the Network's lifecycle,
    just owns it for one task instance.

    Returns a summary dict that the operator pushes to XCom for
    downstream tasks to read.
    """
    reflow.load_pack(pack_path)

    out_path = str(Path(output_dir) / f"{ds}.jsonl")
    Path(out_path).unlink(missing_ok=True)

    counter = {"written": 0, "alerted": 0}
    cv = threading.Condition()

    net = Network()

    # Source
    net.register_actor("tpl_source", reflow.template_actor("api_github_list_issues"))
    net.add_node("source", "tpl_source", config={"state": "open", "filter": "assigned"})

    # Pipeline
    net.register_actor("tpl_extract",   ExtractIssues()._build())
    net.register_actor("tpl_loop",      reflow.template_actor("tpl_loop"))
    net.register_actor("tpl_normalize", IssueNormalize()._build())
    net.add_node("extract",   "tpl_extract")
    net.add_node("each",      "tpl_loop")
    net.add_node("normalize", "tpl_normalize")

    # Routing — high-priority bugs vs everything else.
    net.register_actor("tpl_rule_high", reflow.template_actor("tpl_rules_engine"))
    net.add_node("rule_high", "tpl_rule_high", config=_HIGH_PRIO_RULE)

    # Slack formatter — bridges the matched-issue object to the
    # api_slack_send_message actor's `text` inport.
    net.register_actor("tpl_format", SlackFormatter()._build())
    net.add_node("format", "tpl_format")

    net.register_actor("tpl_slack", reflow.template_actor("api_slack_send_message"))
    net.add_node("slack", "tpl_slack", config={"channel": slack_channel})

    # Alert counter — fans off the same `matched` outport as the
    # formatter, just bumps a counter so the operator knows when to
    # exit.
    class _AlertCounter(Actor):
        component = "alert_counter"
        inports = ["routed"]
        outports = []
        def run(self, ctx):
            with cv:
                counter["alerted"] += 1
                cv.notify_all()
            ctx.done()

    net.register_actor("tpl_alert_count", _AlertCounter()._build())
    net.add_node("alert_count", "tpl_alert_count")

    # Archive sink: append to JSONL and bump the written counter.
    net.register_actor("tpl_archive", JsonlAppender(out_path, counter, cv)._build())
    net.add_node("archive", "tpl_archive")

    # Wiring
    net.add_initial("source", "filter", {"type": "String", "data": "assigned"})
    net.add_connection("source",    "response",  "extract",   "response")
    net.add_connection("extract",   "issues",    "each",      "collection")
    net.add_connection("each",      "item",      "normalize", "item")
    net.add_connection("normalize", "issue",     "rule_high", "data")
    # high_prio matches fan to the formatter (→ slack), the alert
    # counter, and the archive (so we have a record of every alert).
    # Reflow connectors are broadcast: each fires per packet.
    net.add_connection("rule_high", "matched",   "format",      "routed")
    net.add_connection("format",    "text",      "slack",       "text")
    net.add_connection("rule_high", "matched",   "alert_count", "routed")
    # everything else → archive
    net.add_connection("rule_high", "unmatched", "archive",     "data")

    net.start()
    deadline = datetime.now(timezone.utc).timestamp() + timeout_seconds
    with cv:
        # Wait until at least one record has flowed through (writes or
        # alerts), then poll for quiescence over a short idle window.
        while datetime.now(timezone.utc).timestamp() < deadline:
            if counter["written"] + counter["alerted"] == 0:
                cv.wait(timeout=2.0)
                continue
            # Idle for a small window → assume drained.
            initial = (counter["written"], counter["alerted"])
            cv.wait(timeout=1.5)
            if (counter["written"], counter["alerted"]) == initial:
                break
    net.shutdown()

    return {
        "ds": ds,
        "alerted": counter["alerted"],
        "tracked": counter["written"],
        "output_path": out_path,
    }


_HIGH_PRIO_RULE = {
    "rules": {
        "type": "IF",
        "groups": [
            {
                "connector": "OR",
                "rules": [
                    {"field": "labels", "operator": "contains", "value": "priority:high"},
                    {"field": "labels", "operator": "contains", "value": "bug"},
                ],
            },
        ],
        "actions": {"setProperty": [{"key": "branch", "value": "high_prio"}]},
    },
}


def _age_days(iso: str | None) -> float:
    if not iso:
        return 0.0
    try:
        ts = datetime.fromisoformat(iso.replace("Z", "+00:00"))
        return (datetime.now(timezone.utc) - ts).total_seconds() / 86400
    except (ValueError, TypeError):
        return 0.0


# ── Test entry point — run without Airflow ──────────────────────


if __name__ == "__main__":
    import sys
    pack = os.environ.get(
        "REFLOW_PACK", str(Path(__file__).parent / "reflow.pack.api_services.rflpack")
    )
    summary = run_triage(
        ds=datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        pack_path=pack,
        output_dir=str(Path(__file__).parent / "out"),
        slack_channel=os.environ.get("SLACK_CHANNEL", "#ops-triage"),
        timeout_seconds=20.0,
    )
    print(json.dumps(summary, indent=2))
    sys.exit(0)
