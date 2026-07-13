-- 050: Persist the explicit editor action for one unified Run.
-- The action is request-scoped data; no active editor state is re-read during execution.
ALTER TABLE agent_runs ADD COLUMN explicit_action_json TEXT;
