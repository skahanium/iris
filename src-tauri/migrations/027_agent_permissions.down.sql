DROP INDEX IF EXISTS idx_agent_permission_audit_tool;
DROP INDEX IF EXISTS idx_agent_permission_audit_permission;
DROP INDEX IF EXISTS idx_agent_permission_audit_request;
DROP TABLE IF EXISTS agent_permission_audit;

DROP INDEX IF EXISTS idx_agent_permission_grants_skill;
DROP INDEX IF EXISTS idx_agent_permission_grants_scope;
DROP INDEX IF EXISTS idx_agent_permission_grants_permission;
DROP TABLE IF EXISTS agent_permission_grants;
