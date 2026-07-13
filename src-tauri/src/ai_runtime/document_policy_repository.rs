//! Persistent source for the unified document-capability policy matrix.

use std::collections::BTreeMap;

use crate::ai_runtime::policy_decision_engine::{
    CapabilityDecision, DocumentCapability, DocumentPolicy, PolicyDecisionEngine,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Load the complete persisted document policy matrix into the pure policy engine.
///
/// Unknown persisted values fail closed instead of silently becoming an allow.
pub(crate) fn load_policy_decision_engine(db: &Database) -> AppResult<PolicyDecisionEngine> {
    let rows = db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT scope_kind, scope_path, capability, decision
             FROM document_capability_policies
             ORDER BY scope_kind, scope_path, capability",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;

    let mut vault_rules = Vec::new();
    let mut folder_rules = BTreeMap::<String, Vec<(DocumentCapability, CapabilityDecision)>>::new();
    let mut document_rules =
        BTreeMap::<String, Vec<(DocumentCapability, CapabilityDecision)>>::new();
    for (scope_kind, scope_path, capability, decision) in rows {
        let capability = parse_document_capability(&capability)?;
        let decision = parse_capability_decision(&decision)?;
        match scope_kind.as_str() {
            "vault" if scope_path.is_empty() => vault_rules.push((capability, decision)),
            "folder" if !scope_path.trim().is_empty() => {
                folder_rules
                    .entry(scope_path)
                    .or_default()
                    .push((capability, decision));
            }
            "document" if !scope_path.trim().is_empty() => {
                document_rules
                    .entry(scope_path)
                    .or_default()
                    .push((capability, decision));
            }
            _ => return Err(AppError::msg("agent_run_invalid_document_policy")),
        }
    }

    let mut engine = PolicyDecisionEngine::new(DocumentPolicy::from_rules(vault_rules));
    for (path, rules) in folder_rules {
        engine.set_folder_policy(&path, DocumentPolicy::from_rules(rules));
    }
    for (path, rules) in document_rules {
        engine.set_document_policy(&path, DocumentPolicy::from_rules(rules));
    }
    Ok(engine)
}

fn parse_document_capability(value: &str) -> AppResult<DocumentCapability> {
    match value {
        "discover" => Ok(DocumentCapability::Discover),
        "read" => Ok(DocumentCapability::Read),
        "send_to_model" => Ok(DocumentCapability::SendToModel),
        "cite" => Ok(DocumentCapability::Cite),
        "propose_change" => Ok(DocumentCapability::ProposeChange),
        "apply_change" => Ok(DocumentCapability::ApplyChange),
        _ => Err(AppError::msg("agent_run_invalid_document_policy")),
    }
}

fn parse_capability_decision(value: &str) -> AppResult<CapabilityDecision> {
    match value {
        "allow" => Ok(CapabilityDecision::Allow),
        "deny" => Ok(CapabilityDecision::Deny),
        _ => Err(AppError::msg("agent_run_invalid_document_policy")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_vault_folder_and_document_rules_into_the_single_policy_engine() {
        let db = Database::open_in_memory().expect("database");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO document_capability_policies
                 (scope_kind, scope_path, capability, decision)
                 VALUES ('vault', '', 'read', 'allow'),
                 ('folder', 'notes', 'send_to_model', 'deny'),
                 ('document', 'notes/exception.md', 'send_to_model', 'allow')",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        let engine = load_policy_decision_engine(&db).expect("load policy engine");
        assert_eq!(
            engine
                .effective_document_scope("notes/restricted.md")
                .decision_for(DocumentCapability::SendToModel),
            CapabilityDecision::Deny
        );
        assert_eq!(
            engine
                .effective_document_scope("notes/exception.md")
                .decision_for(DocumentCapability::SendToModel),
            CapabilityDecision::Allow
        );
    }
}
