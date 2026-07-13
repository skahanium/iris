//! CEF-only document policy source for classified Agent Runs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ai_runtime::policy_decision_engine::{
    CapabilityDecision, DocumentCapability, DocumentPolicy, PolicyDecisionEngine,
};
use crate::crypto::classified_io;
use crate::crypto::vault_key::VAULT_KEY;
use crate::error::{AppError, AppResult};

const POLICY_FILE_NAME: &str = "document-policies.cef";
const POLICY_SCHEMA_VERSION: u32 = 1;

/// One CEF-persisted document-capability policy rule.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ClassifiedDocumentPolicyRuleInput {
    scope_kind: String,
    scope_path: String,
    capability: String,
    decision: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ClassifiedDocumentPolicyStore {
    version: u32,
    rules: Vec<ClassifiedDocumentPolicyRuleInput>,
}

/// Load the classified policy engine exclusively from encrypted CEF storage.
pub(crate) fn load_classified_policy_decision_engine(
    vault: &Path,
) -> AppResult<PolicyDecisionEngine> {
    let key = require_unlocked_key()?;
    let path = policy_file_path(vault);
    if !path.exists() {
        return Ok(PolicyDecisionEngine::new(DocumentPolicy::allow_all()));
    }
    let ciphertext = fs::read(&path)?;
    let plaintext = classified_io::decrypt_cef(&ciphertext, &key)?;
    let store: ClassifiedDocumentPolicyStore = serde_json::from_slice(&plaintext)?;
    if store.version != POLICY_SCHEMA_VERSION {
        return Err(AppError::msg("invalid classified document policy schema"));
    }
    policy_engine_from_rules(store.rules)
}

fn policy_file_path(vault: &Path) -> PathBuf {
    vault
        .join(".classified")
        .join(".iris-ai")
        .join(POLICY_FILE_NAME)
}

fn require_unlocked_key() -> AppResult<[u8; 32]> {
    let vault_key = VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|error| AppError::msg(format!("VAULT_KEY lock error: {error}")))?;
    if !vault_key.is_unlocked() {
        return Err(AppError::msg("保险库未解锁"));
    }
    Ok(*vault_key.key()?)
}

fn policy_engine_from_rules(
    rules: Vec<ClassifiedDocumentPolicyRuleInput>,
) -> AppResult<PolicyDecisionEngine> {
    let mut vault_rules = Vec::new();
    let mut folder_rules = BTreeMap::<String, Vec<(DocumentCapability, CapabilityDecision)>>::new();
    let mut document_rules =
        BTreeMap::<String, Vec<(DocumentCapability, CapabilityDecision)>>::new();
    for rule in rules {
        let capability = parse_capability(&rule.capability)?;
        let decision = parse_decision(&rule.decision)?;
        match rule.scope_kind.as_str() {
            "vault" if rule.scope_path.is_empty() => vault_rules.push((capability, decision)),
            "folder" if !rule.scope_path.trim().is_empty() => {
                folder_rules
                    .entry(rule.scope_path)
                    .or_default()
                    .push((capability, decision));
            }
            "document" if !rule.scope_path.trim().is_empty() => {
                document_rules
                    .entry(rule.scope_path)
                    .or_default()
                    .push((capability, decision));
            }
            _ => return Err(AppError::msg("invalid classified document policy")),
        }
    }
    let mut engine = PolicyDecisionEngine::new(if vault_rules.is_empty() {
        DocumentPolicy::allow_all()
    } else {
        DocumentPolicy::from_rules(vault_rules)
    });
    for (path, rules) in folder_rules {
        engine.set_folder_policy(&path, DocumentPolicy::from_rules(rules));
    }
    for (path, rules) in document_rules {
        engine.set_document_policy(&path, DocumentPolicy::from_rules(rules));
    }
    Ok(engine)
}

fn parse_capability(value: &str) -> AppResult<DocumentCapability> {
    match value {
        "discover" => Ok(DocumentCapability::Discover),
        "read" => Ok(DocumentCapability::Read),
        "send_to_model" => Ok(DocumentCapability::SendToModel),
        "cite" => Ok(DocumentCapability::Cite),
        "propose_change" => Ok(DocumentCapability::ProposeChange),
        "apply_change" => Ok(DocumentCapability::ApplyChange),
        _ => Err(AppError::msg("invalid classified document policy")),
    }
}

fn parse_decision(value: &str) -> AppResult<CapabilityDecision> {
    match value {
        "allow" => Ok(CapabilityDecision::Allow),
        "deny" => Ok(CapabilityDecision::Deny),
        _ => Err(AppError::msg("invalid classified document policy")),
    }
}
