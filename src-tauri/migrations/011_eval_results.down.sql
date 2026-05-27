-- Rollback: drop evaluation results table.

DROP INDEX IF EXISTS idx_eval_results_created;
DROP TABLE IF EXISTS ai_eval_results;
