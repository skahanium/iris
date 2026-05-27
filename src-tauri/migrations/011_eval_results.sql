-- Evaluation results table (§13)
-- Stores evaluation run results for retrieval, generation, and safety metrics.

CREATE TABLE IF NOT EXISTS ai_eval_results (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          TEXT NOT NULL UNIQUE,
    metric          TEXT NOT NULL,
    score           REAL NOT NULL,
    total_cases     INTEGER NOT NULL,
    passed_cases    INTEGER NOT NULL,
    failed_cases    TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_eval_results_created
    ON ai_eval_results(created_at DESC);
