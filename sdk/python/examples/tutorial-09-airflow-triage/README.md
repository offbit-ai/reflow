# Tutorial 09 — Reflow inside an Airflow PythonOperator (Python)

Runnable code for [Real-world Reflow 09: Reflow inside an Airflow
PythonOperator](../../../../docs/tutorials/real-world/09-airflow-triage.md).

A daily issue-triage pipeline. Airflow handles the calendar
(schedule, backfill, retry, UI). Reflow handles the actor graph
(read GitHub, route via rules, post Slack, archive JSONL).

```
sdk/python/examples/tutorial-09-airflow-triage/
├── pipeline.py        # the Reflow Network — function the operator calls
├── dags/
│   └── triage.py      # Airflow DAG file ($AIRFLOW_HOME/dags/...)
└── reflow.pack.api_services.rflpack   # symlink — see below
```

## Setup

```sh
pip install 'offbit-reflow>=0.2.9' 'apache-airflow>=2.10' 'apache-airflow-providers-postgres'

PACK=https://github.com/offbit-ai/reflow/releases/download/pack-v0.2.5
TRIPLE=$(uname -m)-apple-darwin
curl -LO "$PACK/reflow.pack.api_services-0.2.0-$TRIPLE.rflpack"
mv reflow.pack.api_services-0.2.0-*.rflpack reflow.pack.api_services.rflpack
```

## Iterate without Airflow

The Reflow side has no Airflow dependency — `pipeline.py` ships a
`__main__` entry that runs the network directly:

```sh
export GITHUB_API_KEY=$(gh auth token)        # any PAT with repo scope
export SLACK_API_KEY=xoxb-...                  # bot token (optional for testing —
                                               # 401s emit on `error` outport)
export SLACK_CHANNEL=#ops-triage
python3 pipeline.py
# {"ds": "2026-04-28", "alerted": 2, "tracked": 9, "output_path": "..."}
```

Useful for iterating on the actor graph before deploying the DAG.

## Deploy as an Airflow DAG

1. Drop `pipeline.py` and `dags/triage.py` into your Airflow
   plugin / DAGs folder. Both files travel together — `triage.py`
   imports `pipeline.run_triage`.
2. Set Airflow Variables (Admin → Variables, or via CLI):
   ```sh
   airflow variables set REFLOW_GITHUB_API_KEY ghp_...
   airflow variables set REFLOW_SLACK_API_KEY  xoxb-...
   airflow variables set REFLOW_PACK_PATH      /opt/airflow/dags/reflow.pack.api_services.rflpack
   airflow variables set REFLOW_OUTPUT_DIR     /var/lib/reflow-triage
   airflow variables set REFLOW_SLACK_CHANNEL  '#ops-triage'
   ```
3. Add a Postgres connection for the metrics sink:
   ```sh
   airflow connections add reflow_metrics --conn-uri 'postgres://...'
   ```
4. Refresh the scheduler. The DAG appears as `reflow_issue_triage`
   on the daily schedule. With `catchup=True`, Airflow will run
   every missing date since `start_date`.

Backfill a specific window:

```sh
airflow dags backfill -s 2024-12-01 -e 2024-12-31 reflow_issue_triage
```

## Why the pairing earns its weight

| Concern | Owned by |
|---|---|
| Cron, backfill, retry, calendar UI | Airflow |
| Encrypted credentials store | Airflow |
| Per-actor concurrency, bounded backpressure, broadcast wiring | Reflow |
| The 6,700-actor catalog (api_github_*, api_slack_*, …) | Reflow |
| Conditional routing as JSON config (tpl_rules_engine) | Reflow |

Don't try to model the fan-out from `tpl_rules_engine.matched` →
`SlackFormatter` + `AlertCounter` as Airflow tasks — that'd burn
~6 task instances per matched issue. Reflow handles per-tick
scheduling natively; Airflow handles the calendar.
