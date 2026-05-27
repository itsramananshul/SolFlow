-- Phase C C.3 — scheduling MVP.
--
-- One row per registered trigger. Both Timer and Event triggers
-- live here; `trigger_json` carries the discriminated payload.
--
-- The scheduler tick polls `WHERE enabled=1 AND next_fire_at <= now`
-- to find Timer triggers due for firing. Event triggers don't use
-- `next_fire_at` (always NULL) and are looked up by path inside
-- the webhook handler.
--
-- Soft-delete is intentionally NOT modeled: schedules go away
-- entirely on cancel. Run history retains the trigger that created
-- each run, so audit doesn't depend on the schedule still existing.

CREATE TABLE IF NOT EXISTS schedules (
    id            TEXT PRIMARY KEY NOT NULL,
    workflow_id   TEXT NOT NULL REFERENCES workflows(id),
    trigger_json  TEXT NOT NULL,                 -- serde JSON of RunTrigger
    enabled       INTEGER NOT NULL DEFAULT 1,    -- 0 / 1
    next_fire_at  INTEGER,                       -- ms since epoch; NULL for Event triggers
    created_at    INTEGER NOT NULL
);

-- Hot path for the scheduler tick: "what timers are due now".
CREATE INDEX IF NOT EXISTS idx_schedules_due
    ON schedules(enabled, next_fire_at)
    WHERE next_fire_at IS NOT NULL;

-- Workflow-scoped lookup for the GET /workflows/:id/schedules endpoint.
CREATE INDEX IF NOT EXISTS idx_schedules_workflow
    ON schedules(workflow_id);
