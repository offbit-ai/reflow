"""Airflow DAG: daily issue triage.

Schedule: daily. Each run kicks one Reflow Network whose body
mirrors tutorial 07 — read GitHub issues, route via tpl_rules_engine,
post Slack alerts for high-priority bugs, archive everything else
into a JSONL row keyed by `{ds}`.

The Airflow side handles cron, retries, backfill, the calendar UI,
and the credentials store. The Reflow side handles the actor graph.
This DAG file is what production Airflow installations would
deploy under their `dags_folder/`.

Place this file under `$AIRFLOW_HOME/dags/` and refresh the
scheduler. Backfill: `airflow dags backfill -s 2024-01-01 -e 2024-01-31`.
"""

from __future__ import annotations

import json
import sys
from datetime import datetime
from pathlib import Path

from airflow.decorators import dag, task
from airflow.models import Variable
from airflow.providers.postgres.operators.postgres import PostgresOperator

# Make `pipeline.py` importable from the parent directory. In a real
# Airflow deployment, ship `pipeline.py` as part of the same plugin /
# wheel and import it normally.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from pipeline import run_triage  # noqa: E402


@dag(
    dag_id="reflow_issue_triage",
    schedule="@daily",
    start_date=datetime(2024, 1, 1),
    catchup=True,
    max_active_runs=1,
    tags=["reflow", "triage"],
    doc_md=__doc__,
)
def triage_pipeline():
    @task(retries=2, retry_exponential_backoff=True)
    def triage(ds: str | None = None, **ctx) -> dict:
        """Run one day's Reflow triage. Returns the summary as XCom.

        Credentials come from Airflow Variables. The PythonOperator's
        ENV-injection of GITHUB_API_KEY / SLACK_API_KEY is what the
        api_github_* / api_slack_* templates read internally — so the
        Reflow code is identical to a stand-alone script; only how
        the env vars get populated changes.
        """
        import os

        os.environ["GITHUB_API_KEY"] = Variable.get("REFLOW_GITHUB_API_KEY")
        os.environ["SLACK_API_KEY"]  = Variable.get("REFLOW_SLACK_API_KEY")

        return run_triage(
            ds=ds or ctx["ds"],
            pack_path=Variable.get("REFLOW_PACK_PATH"),
            output_dir=Variable.get("REFLOW_OUTPUT_DIR", default_var="/tmp/reflow-triage"),
            slack_channel=Variable.get("REFLOW_SLACK_CHANNEL", default_var="#ops-triage"),
            timeout_seconds=600.0,
        )

    record = PostgresOperator(
        task_id="record_run",
        postgres_conn_id="reflow_metrics",
        sql=(
            "INSERT INTO triage_runs (ds, alerted, tracked, output_path) "
            "VALUES ('{{ ds }}', "
            "        {{ ti.xcom_pull(task_ids='triage')['alerted'] }}, "
            "        {{ ti.xcom_pull(task_ids='triage')['tracked'] }}, "
            "        '{{ ti.xcom_pull(task_ids='triage')['output_path'] }}') "
            "ON CONFLICT (ds) DO UPDATE SET "
            "  alerted = EXCLUDED.alerted, "
            "  tracked = EXCLUDED.tracked;"
        ),
    )

    triage() >> record


triage_pipeline()
