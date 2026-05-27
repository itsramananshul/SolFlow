-- C.2 initial schema. See PHASE_C_ARCHITECTURE.md §6.1.

CREATE TABLE IF NOT EXISTS workflows (
    id                TEXT PRIMARY KEY NOT NULL,
    content_hash      TEXT NOT NULL,
    bytecode          BLOB NOT NULL,
    instruction_spans BLOB NOT NULL,
    source            TEXT,
    name              TEXT NOT NULL,
    description       TEXT,
    created_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflows_content_hash
    ON workflows(content_hash);

CREATE TABLE IF NOT EXISTS runs (
    id               TEXT PRIMARY KEY NOT NULL,
    workflow_id      TEXT NOT NULL REFERENCES workflows(id),
    status           TEXT NOT NULL CHECK (
                          status IN ('Queued','Running','Succeeded','Failed','Cancelled')),
    trigger_json     TEXT NOT NULL,
    inputs_json      TEXT NOT NULL DEFAULT '{}',
    output_json      TEXT,
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    started_at       INTEGER,
    completed_at     INTEGER,
    created_at       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_workflow_id ON runs(workflow_id);
CREATE INDEX IF NOT EXISTS idx_runs_status      ON runs(status);
CREATE INDEX IF NOT EXISTS idx_runs_created_at  ON runs(created_at);

-- run_events + schedules deferred to C.5 / C.3.
