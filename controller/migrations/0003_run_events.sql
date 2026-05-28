-- Phase C C.5 — event log.
--
-- One row per RunEvent emitted during a run. `payload_json` is
-- the full serde-serialized event (denormalized vs splitting per
-- variant) so adding new variants doesn't require a schema
-- migration. `kind` mirrors the serde tag for cheap filtering
-- in SQL ("show me all Failed events for this run").
--
-- Composite primary key (run_id, seq) guarantees monotonicity
-- per run AND gives the SSE replay query its index for free.

CREATE TABLE IF NOT EXISTS run_events (
    run_id        TEXT NOT NULL REFERENCES runs(id),
    seq           INTEGER NOT NULL,
    ts            INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    PRIMARY KEY (run_id, seq)
);

-- Time-ordered queries (e.g. "last 50 events across all runs"
-- for a debug dashboard).
CREATE INDEX IF NOT EXISTS idx_run_events_ts
    ON run_events(ts DESC);
