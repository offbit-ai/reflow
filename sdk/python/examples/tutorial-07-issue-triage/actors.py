"""Custom actors for the issue-triage workflow.

Most of the graph is catalog templates instantiated by id —
`tpl_data_emit`, `tpl_loop`, `tpl_rules_engine` (the routing
brain), `tpl_file_save`, plus the `api_*` actors when run with
real credentials. These small custom actors fill the gaps the
catalog can't anticipate:

- ``ExtractIssues`` peels the `body` array out of an HTTP response
  envelope so downstream `tpl_loop` sees a bare list of issues.
- ``IssueNormalize`` flattens each issue into a shape the rules
  engine can match against — labels as a string array, age in
  days as a number.
- ``JsonlAppender`` appends one record per tick to a growing file
  (`tpl_file_save` overwrites; we want append-mode).
- ``ConsoleSink`` is the no-credentials fallback for the action
  leaves: prints what *would* have been Slack-posted.
"""

from __future__ import annotations

from datetime import datetime, timezone

from offbit_reflow import Actor, Message


class ExtractIssues(Actor):
    """Strips the `{status, headers, body}` HTTP envelope down to the
    body array and forwards as a Reflow Array message.

    Catalog API actors emit `response = {status, headers, body}`. The
    fixture-mode `tpl_data_emit` is configured to emit the same shape,
    so swapping between modes is a one-line change.
    """

    component = "extract_issues"
    inports = ["response"]
    outports = ["issues"]

    def run(self, ctx) -> None:
        envelope = ctx.inputs["response"]["data"]
        body = envelope.get("body") if isinstance(envelope, dict) else envelope
        if not isinstance(body, list):
            ctx.fail(f"expected list of issues, got {type(body).__name__}")
            return
        ctx.done({"issues": Message.array(body)})


class IssueNormalize(Actor):
    """Flattens one issue into a shape `tpl_rules_engine` can match
    against:

      labels        : list[str]       — names only, dropping the GitHub
                                         color/id fields the rules don't
                                         care about
      comments      : int             — pass-through
      has_assignee  : bool            — convenience for rules
      age_days      : float           — derived from created_at
      number, title, url, …           — preserved for the action sinks

    The rules engine's `contains` works on `Value::Array(...)` against a
    bare value, so labels-as-strings is the right shape.
    """

    component = "issue_normalize"
    inports = ["item"]
    outports = ["issue"]

    def run(self, ctx) -> None:
        # tpl_loop wraps each item as {"value": <issue>, "index": <int>}
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


class ConsoleSink(Actor):
    """Stand-in for `api_slack_send_message` / GitHub comment posting
    when credentials are absent. Prints `[label] #N title  url` so
    the audit log is readable in the test output.
    """

    component = "console_sink"
    inports = ["routed"]
    outports = []

    def __init__(self, label: str) -> None:
        self.label = label

    def run(self, ctx) -> None:
        issue = ctx.inputs["routed"]["data"]
        n = issue.get("number") if isinstance(issue, dict) else "?"
        title = issue.get("title") if isinstance(issue, dict) else str(issue)
        url = issue.get("url") if isinstance(issue, dict) else ""
        print(f"[{self.label:<11}] #{n} {title}  {url}")
        ctx.done()


class JsonlAppender(Actor):
    """Append-mode JSONL writer. The catalog `tpl_file_save` writes
    the whole file in one shot — fine for blob output, wrong for an
    audit log that grows per-tick.
    """

    component = "jsonl_appender"
    inports = ["data"]
    outports = []

    def __init__(self, path: str) -> None:
        self.path = path

    def run(self, ctx) -> None:
        import json as _json
        from pathlib import Path
        Path(self.path).parent.mkdir(parents=True, exist_ok=True)
        row = ctx.inputs["data"]["data"]
        with open(self.path, "a") as f:
            f.write(_json.dumps(row, separators=(",", ":")) + "\n")
        ctx.done()


def _age_days(iso: str | None) -> float:
    if not iso:
        return 0.0
    try:
        ts = datetime.fromisoformat(iso.replace("Z", "+00:00"))
        return (datetime.now(timezone.utc) - ts).total_seconds() / 86400
    except (ValueError, TypeError):
        return 0.0
