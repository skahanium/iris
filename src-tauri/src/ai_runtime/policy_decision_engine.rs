//! Pure policy-kernel rules for the future Agent Run control plane.
//!
//! This phase deliberately does not connect the kernel to the legacy tool
//! dispatcher. It provides deterministic document-scope and material-role
//! resolution that the unified pipeline will consume in a later phase.

use std::array;
use std::collections::BTreeMap;

/// A single operation that document policy can independently permit or deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DocumentCapability {
    /// Discover that a document exists in a scope.
    Discover,
    /// Read the document's content.
    Read,
    /// Include the document's content in a model request.
    SendToModel,
    /// Cite the document as evidence.
    Cite,
    /// Produce a proposed change for the document.
    ProposeChange,
    /// Apply a user-confirmed change to the document.
    ApplyChange,
}

impl DocumentCapability {
    const ALL: [Self; 6] = [
        Self::Discover,
        Self::Read,
        Self::SendToModel,
        Self::Cite,
        Self::ProposeChange,
        Self::ApplyChange,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Discover => 0,
            Self::Read => 1,
            Self::SendToModel => 2,
            Self::Cite => 3,
            Self::ProposeChange => 4,
            Self::ApplyChange => 5,
        }
    }
}

/// Explicit document-policy outcome for one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityDecision {
    /// The capability is permitted by the effective document scope.
    Allow,
    /// The capability is blocked and cannot be elevated by an explicit `@`.
    Deny,
}

/// One rule declared at the vault, folder, or document level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocumentCapabilityRule {
    /// Capability governed by this rule.
    pub capability: DocumentCapability,
    /// Explicit allow or deny declaration.
    pub decision: CapabilityDecision,
}

/// Rules declared at one policy level.
///
/// A capability omitted at a level inherits from the next less-specific level.
/// If a level contains both decisions for the same capability, deny wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DocumentPolicy {
    rules: Vec<DocumentCapabilityRule>,
}

impl DocumentPolicy {
    /// Creates the ordinary vault policy, which permits all document capabilities.
    pub(crate) fn allow_all() -> Self {
        Self::from_rules(
            DocumentCapability::ALL
                .into_iter()
                .map(|capability| (capability, CapabilityDecision::Allow)),
        )
    }

    /// Creates a policy from explicit rules without silently collapsing conflicts.
    pub(crate) fn from_rules(
        rules: impl IntoIterator<Item = (DocumentCapability, CapabilityDecision)>,
    ) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|(capability, decision)| DocumentCapabilityRule {
                    capability,
                    decision,
                })
                .collect(),
        }
    }

    fn explicit_decision_for(&self, capability: DocumentCapability) -> Option<CapabilityDecision> {
        let mut allow_found = false;

        for rule in &self.rules {
            if rule.capability != capability {
                continue;
            }
            if rule.decision == CapabilityDecision::Deny {
                return Some(CapabilityDecision::Deny);
            }
            allow_found = true;
        }

        allow_found.then_some(CapabilityDecision::Allow)
    }
}

/// Policy level that supplied the final decision for a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DocumentPolicySource {
    /// A vault default supplied the decision.
    Vault,
    /// A folder policy supplied the decision.
    Folder(String),
    /// A document-specific policy supplied the decision.
    Document(String),
}

/// Fully resolved, per-capability scope for a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveDocumentScope {
    decisions: [CapabilityDecision; 6],
    sources: [DocumentPolicySource; 6],
}

impl EffectiveDocumentScope {
    /// Returns the resolved decision for one capability.
    pub(crate) fn decision_for(&self, capability: DocumentCapability) -> CapabilityDecision {
        self.decisions[capability.index()]
    }

    /// Returns the policy level from which one capability was resolved.
    pub(crate) fn source_for(&self, capability: DocumentCapability) -> &DocumentPolicySource {
        &self.sources[capability.index()]
    }

    /// Returns every resolved capability decision in stable capability order.
    pub(crate) fn decisions(&self) -> [(DocumentCapability, CapabilityDecision); 6] {
        DocumentCapability::ALL.map(|capability| (capability, self.decision_for(capability)))
    }
}

/// Role that determines how a folder's material may be used as evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MaterialRole {
    /// Normative material used as a governing authority.
    Authority,
    /// A style or structural sample.
    Exemplar,
    /// Supporting reference material.
    Reference,
    /// Material only consulted for lookup.
    Lookup,
}

/// Safe diagnostic emitted when a persisted material role must be downgraded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PolicyDiagnostic {
    /// An unrecognized persisted role was interpreted as least-privileged lookup.
    UnknownMaterialRoleDowngraded { value: String },
}

/// Result of parsing a persisted material role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaterialRoleResolution {
    /// Effective role after canonical, legacy, or safe-fallback parsing.
    pub role: MaterialRole,
    /// Present only when the source configuration needs repair or review.
    pub diagnostic: Option<PolicyDiagnostic>,
}

/// Independent, pure policy kernel for document scope and material-role resolution.
///
/// This type intentionally has no database, dispatcher, or legacy scene dependency.
#[derive(Debug, Clone)]
pub(crate) struct PolicyDecisionEngine {
    vault_default: DocumentPolicy,
    folder_policies: BTreeMap<String, DocumentPolicy>,
    document_policies: BTreeMap<String, DocumentPolicy>,
}

impl PolicyDecisionEngine {
    /// Creates an engine with the supplied vault-level default policy.
    pub(crate) fn new(vault_default: DocumentPolicy) -> Self {
        Self {
            vault_default,
            folder_policies: BTreeMap::new(),
            document_policies: BTreeMap::new(),
        }
    }

    /// Replaces the policy for one vault-relative folder.
    pub(crate) fn set_folder_policy(&mut self, folder_path: &str, policy: DocumentPolicy) {
        self.folder_policies
            .insert(normalize_relative_path(folder_path), policy);
    }

    /// Replaces the policy for one vault-relative document.
    pub(crate) fn set_document_policy(&mut self, document_path: &str, policy: DocumentPolicy) {
        self.document_policies
            .insert(normalize_relative_path(document_path), policy);
    }

    /// Resolves the six independent document capabilities for a document path.
    pub(crate) fn effective_document_scope(&self, document_path: &str) -> EffectiveDocumentScope {
        let document_path = normalize_relative_path(document_path);
        let applicable_folders = self.applicable_folder_policies(&document_path);
        let document_policy = self.document_policies.get(&document_path);

        let decisions = array::from_fn(|index| {
            let capability = DocumentCapability::ALL[index];
            self.resolve_capability(
                capability,
                &document_path,
                &applicable_folders,
                document_policy,
            )
            .0
        });
        let sources = array::from_fn(|index| {
            let capability = DocumentCapability::ALL[index];
            self.resolve_capability(
                capability,
                &document_path,
                &applicable_folders,
                document_policy,
            )
            .1
        });

        EffectiveDocumentScope { decisions, sources }
    }

    /// Parses canonical and legacy folder roles without granting unknown roles authority.
    pub(crate) fn parse_material_role(value: &str) -> MaterialRoleResolution {
        let role = match value {
            "authority" | "regulation" => MaterialRole::Authority,
            "exemplar" => MaterialRole::Exemplar,
            "reference" => MaterialRole::Reference,
            "lookup" | "general" => MaterialRole::Lookup,
            _ => {
                return MaterialRoleResolution {
                    role: MaterialRole::Lookup,
                    diagnostic: Some(PolicyDiagnostic::UnknownMaterialRoleDowngraded {
                        value: value.to_string(),
                    }),
                };
            }
        };

        MaterialRoleResolution {
            role,
            diagnostic: None,
        }
    }

    fn applicable_folder_policies(&self, document_path: &str) -> Vec<(&str, &DocumentPolicy)> {
        let mut policies = self
            .folder_policies
            .iter()
            .filter(|(folder_path, _)| folder_applies_to_document(folder_path, document_path))
            .map(|(folder_path, policy)| (folder_path.as_str(), policy))
            .collect::<Vec<_>>();
        policies.sort_by_key(|(folder_path, _)| folder_path.len());
        policies
    }

    fn resolve_capability(
        &self,
        capability: DocumentCapability,
        document_path: &str,
        applicable_folders: &[(&str, &DocumentPolicy)],
        document_policy: Option<&DocumentPolicy>,
    ) -> (CapabilityDecision, DocumentPolicySource) {
        let mut resolution = self
            .vault_default
            .explicit_decision_for(capability)
            .map(|decision| (decision, DocumentPolicySource::Vault))
            .unwrap_or((CapabilityDecision::Allow, DocumentPolicySource::Vault));

        for (folder_path, folder_policy) in applicable_folders {
            if let Some(decision) = folder_policy.explicit_decision_for(capability) {
                resolution = (
                    decision,
                    DocumentPolicySource::Folder((*folder_path).to_string()),
                );
            }
        }

        if let Some(decision) =
            document_policy.and_then(|policy| policy.explicit_decision_for(capability))
        {
            resolution = (
                decision,
                DocumentPolicySource::Document(document_path.to_string()),
            );
        }

        resolution
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.trim_matches('/').replace('\\', "/")
}

fn folder_applies_to_document(folder_path: &str, document_path: &str) -> bool {
    !folder_path.is_empty()
        && document_path.starts_with(folder_path)
        && document_path
            .as_bytes()
            .get(folder_path.len())
            .is_some_and(|separator| *separator == b'/')
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityDecision, DocumentCapability, DocumentPolicy, DocumentPolicySource, MaterialRole,
        PolicyDecisionEngine, PolicyDiagnostic,
    };

    fn policy(rules: &[(DocumentCapability, CapabilityDecision)]) -> DocumentPolicy {
        DocumentPolicy::from_rules(rules.iter().copied())
    }

    #[test]
    fn vault_defaults_allow_each_document_capability() {
        let engine = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        let scope = engine.effective_document_scope("notes/brief.md");

        assert_eq!(
            scope.decisions(),
            [
                (DocumentCapability::Discover, CapabilityDecision::Allow),
                (DocumentCapability::Read, CapabilityDecision::Allow),
                (DocumentCapability::SendToModel, CapabilityDecision::Allow),
                (DocumentCapability::Cite, CapabilityDecision::Allow),
                (DocumentCapability::ProposeChange, CapabilityDecision::Allow),
                (DocumentCapability::ApplyChange, CapabilityDecision::Allow),
            ]
        );
    }

    #[test]
    fn closest_folder_overrides_only_its_configured_capability() {
        let mut engine = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        engine.set_folder_policy(
            "notes",
            policy(&[(DocumentCapability::SendToModel, CapabilityDecision::Deny)]),
        );
        engine.set_folder_policy(
            "notes/private",
            policy(&[(DocumentCapability::Read, CapabilityDecision::Deny)]),
        );

        let scope = engine.effective_document_scope("notes/private/plan.md");

        assert_eq!(
            scope.decision_for(DocumentCapability::Read),
            CapabilityDecision::Deny
        );
        assert_eq!(
            scope.decision_for(DocumentCapability::SendToModel),
            CapabilityDecision::Deny
        );
        assert_eq!(
            scope.decision_for(DocumentCapability::Cite),
            CapabilityDecision::Allow
        );
    }

    #[test]
    fn document_override_wins_over_folder_for_its_own_capability() {
        let mut engine = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        engine.set_folder_policy(
            "notes",
            policy(&[(DocumentCapability::ApplyChange, CapabilityDecision::Deny)]),
        );
        engine.set_document_policy(
            "notes/draft.md",
            policy(&[(DocumentCapability::ApplyChange, CapabilityDecision::Allow)]),
        );

        let scope = engine.effective_document_scope("notes/draft.md");

        assert_eq!(
            scope.decision_for(DocumentCapability::ApplyChange),
            CapabilityDecision::Allow
        );
        assert_eq!(
            scope.source_for(DocumentCapability::ApplyChange),
            &DocumentPolicySource::Document("notes/draft.md".to_string())
        );
    }

    #[test]
    fn deny_wins_when_one_policy_level_contains_conflicting_rules() {
        let engine = PolicyDecisionEngine::new(policy(&[
            (DocumentCapability::Read, CapabilityDecision::Allow),
            (DocumentCapability::Read, CapabilityDecision::Deny),
        ]));

        assert_eq!(
            engine
                .effective_document_scope("notes/brief.md")
                .decision_for(DocumentCapability::Read),
            CapabilityDecision::Deny
        );
    }

    #[test]
    fn folder_match_requires_a_path_boundary() {
        let mut engine = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        engine.set_folder_policy(
            "work",
            policy(&[(DocumentCapability::Read, CapabilityDecision::Deny)]),
        );

        assert_eq!(
            engine
                .effective_document_scope("workshop/brief.md")
                .decision_for(DocumentCapability::Read),
            CapabilityDecision::Allow
        );
    }

    #[test]
    fn material_roles_preserve_canonical_values_and_safely_map_legacy_values() {
        assert_eq!(
            PolicyDecisionEngine::parse_material_role("authority").role,
            MaterialRole::Authority
        );
        assert_eq!(
            PolicyDecisionEngine::parse_material_role("regulation").role,
            MaterialRole::Authority
        );
        assert_eq!(
            PolicyDecisionEngine::parse_material_role("general").role,
            MaterialRole::Lookup
        );
    }

    #[test]
    fn unknown_material_role_downgrades_to_lookup_with_diagnostic() {
        let resolution = PolicyDecisionEngine::parse_material_role("administrator");

        assert_eq!(resolution.role, MaterialRole::Lookup);
        assert_eq!(
            resolution.diagnostic,
            Some(PolicyDiagnostic::UnknownMaterialRoleDowngraded {
                value: "administrator".to_string(),
            })
        );
    }
}
