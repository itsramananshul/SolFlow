-- Phase C C.6 c89 — lifecycle expansion.
--
-- Two changes to the `runs` table:
--   1. Relax the `status` CHECK constraint to permit four new
--      lifecycle states added in c89:
--        Starting    — between Queued and Running
--        Cancelling  — between Running and Cancelled
--        TimedOut    — terminal: wall-clock budget exhausted
--        Rejected    — terminal: controller refused to enqueue
--   2. Add a `cancel_requested` column the RunManager (c91)
--      uses to coordinate cancellation across restarts:
--      cancellation persists, the worker honors it before
--      transitioning Queued → Starting.
--
-- SQLite can't ALTER a CHECK constraint in-place, so we rebuild
-- the table. Schema is otherwise identical to 0001's runs table
-- plus the new column. Existing data is preserved verbatim
-- (status strings already in the table are all still valid
-- under the new CHECK list).

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS runs_c6 (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    status TEXT NOT NULL CHECK (status IN (
        'Queued','Starting','Running','Cancelling',
        'Cancelled','Succeeded','Failed','TimedOut','Rejected'
    )),
    trigger_json TEXT NOT NULL,
    inputs_json TEXT NOT NULL DEFAULT '{}',
    output_json TEXT,
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    started_at INTEGER,
    completed_at INTEGER,
    created_at INTEGER NOT NULL,
    cancel_requested INTEGER NOT NULL DEFAULT 0
);

-- Copy over rows from the existing table. The cancel_requested
-- column gets its default (0). Idempotent against fresh DBs:
-- on a brand-new install the SELECT yields nothing.
INSERT INTO runs_c6 (
    id, workflow_id, status, trigger_json, inputs_json,
    output_json, diagnostics_json, started_at, completed_at,
    created_at
)
SELECT
    id, workflow_id, status, trigger_json, inputs_json,
    output_json, diagnostics_json, started_at, completed_at,
    created_at
FROM runs
WHERE NOT EXISTS (SELECT 1 FROM runs_c6 WHERE runs_c6.id = runs.id);

DROP TABLE runs;
ALTER TABLE runs_c6 RENAME TO runs;

COMMIT;

PRAGMA foreign_keys = ON;
