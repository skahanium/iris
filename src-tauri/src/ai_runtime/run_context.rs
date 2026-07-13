//! Explicit-reference-only Run context assembly.
//!
//! Context is constructed after Request Intake from immutable facts stored for a
//! single Run. It never accepts client excerpts, reads active editor state, or
//! scans a vault for related documents.

use std::path::Path;

use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole,
};
use crate::ai_runtime::agent_run_repository::{AgentRunRepository, StoredExplicitReference};
use crate::ai_runtime::domain_executor::{
    AuthorizedDomainMaterial, DomainExecutionPlan, DomainExecutor, DomainMaterialRole,
};
use crate::ai_runtime::run_contract::ExecutionEnvelope;
use crate::error::{AppError, AppResult};

const MAX_EXPLICIT_MATERIALS: usize = 12;
const MAX_EXPLICIT_MATERIAL_CHARS: usize = 12_000;
const MAX_TOTAL_MATERIAL_CHARS: usize = 32_000;

/// One authorized local source body held only while building a Provider request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunContextMaterial {
    /// Policy-assigned role for this already-authorized source.
    pub(crate) role: DomainMaterialRole,
    pub(crate) source_path: String,
    pub(crate) content_hash: String,
    pub(crate) source_span_start: i64,
    pub(crate) source_span_end: i64,
    pub(crate) content: String,
}

/// The transient, single-Run context sent to a Provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunContext {
    pub(crate) session_id: i64,
    pub(crate) message_seq_first: i64,
    pub(crate) user_message: String,
    pub(crate) envelope: ExecutionEnvelope,
    pub(crate) materials: Vec<RunContextMaterial>,
}

impl RunContext {
    /// Resolve the stateless domain plan from this Run's persisted envelope and authorized data.
    pub(crate) fn domain_plan(&self) -> DomainExecutionPlan {
        let materials = self
            .materials
            .iter()
            .map(|material| AuthorizedDomainMaterial {
                role: material.role,
                label: material.source_path.clone(),
                content: material.content.clone(),
            })
            .collect::<Vec<_>>();
        DomainExecutor::plan(&self.envelope, &self.user_message, &materials, &[])
    }

    /// Render a prompt using one already-resolved domain plan for the same Run.
    pub(crate) fn prompt_with_domain_plan(&self, plan: &DomainExecutionPlan) -> String {
        let mut prompt = plan.prompt_instructions.clone();
        prompt.push_str("\n\n用户请求：\n");
        prompt.push_str(&self.user_message);
        if !plan.rendered_context.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&plan.rendered_context);
        }
        prompt
    }
}

/// Assembles normal-domain context from one persisted Run and one vault.
pub(crate) struct RunContextAssembler;

impl RunContextAssembler {
    /// Read only explicit references persisted with the Run, then validate every source.
    pub(crate) fn assemble(
        db: &crate::storage::db::Database,
        vault: Option<&Path>,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<RunContext> {
        let input = AgentRunRepository::prompt_input_for_session(db, session_key, run_id)?
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
        if input.explicit_references.len() > MAX_EXPLICIT_MATERIALS {
            return Err(AppError::msg("agent_run_invalid_explicit_reference"));
        }

        let envelope = AgentRunRepository::policy_request_for_session(db, session_key, run_id)?
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?
            .envelope;
        let mut materials = Vec::with_capacity(input.explicit_references.len());
        let mut total_chars = 0usize;
        for reference in &input.explicit_references {
            let material = resolve_explicit_reference(vault, reference)?;
            total_chars = total_chars.saturating_add(material.content.chars().count());
            if total_chars > MAX_TOTAL_MATERIAL_CHARS {
                return Err(AppError::msg("agent_run_context_too_large"));
            }
            materials.push(material);
        }
        Ok(RunContext {
            session_id: input.session_id,
            message_seq_first: input.message_seq_first,
            user_message: input.user_message,
            envelope,
            materials,
        })
    }

    /// Register only material metadata in the normal-domain evidence ledger.
    /// Source bodies remain transient in the assembled Provider prompt.
    pub(crate) fn register_evidence(
        db: &crate::storage::db::Database,
        run_id: &str,
        context: &RunContext,
    ) -> AppResult<Vec<i64>> {
        context
            .materials
            .iter()
            .map(|material| {
                AgentEvidenceRepository::register_local(
                    db,
                    LocalEvidenceInput {
                        session_id: context.session_id,
                        run_id: run_id.to_string(),
                        message_seq_first: context.message_seq_first,
                        material_role: evidence_material_role(material.role),
                        title: material.source_path.clone(),
                        source_path: material.source_path.clone(),
                        source_span_start: material.source_span_start,
                        source_span_end: material.source_span_end,
                        heading_path: None,
                        content_hash: material.content_hash.clone(),
                        retrieval_reason: Some("explicit_reference".to_string()),
                        score: None,
                    },
                )
                .map(|registered| registered.evidence_id)
            })
            .collect()
    }
}

fn evidence_material_role(role: DomainMaterialRole) -> MaterialRole {
    match role {
        DomainMaterialRole::Authority => MaterialRole::Authority,
        DomainMaterialRole::Exemplar => MaterialRole::Exemplar,
        DomainMaterialRole::Reference => MaterialRole::Reference,
        DomainMaterialRole::Lookup => MaterialRole::Lookup,
    }
}
fn resolve_explicit_reference(
    vault: Option<&Path>,
    reference: &StoredExplicitReference,
) -> AppResult<RunContextMaterial> {
    if reference.stale || reference.invalid_reason.is_some() {
        return Err(AppError::msg("agent_run_invalid_explicit_reference"));
    }
    let path = reference
        .file_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::msg("agent_run_invalid_explicit_reference"))?;
    let vault = vault.ok_or_else(|| AppError::msg("agent_run_invalid_explicit_reference"))?;
    let resolved = crate::storage::paths::validate_user_note_relative_path(vault, path)
        .map_err(|_| AppError::msg("agent_run_invalid_explicit_reference"))?;
    let full_content = crate::storage::paths::read_file_lossy(&resolved)
        .map_err(|_| AppError::msg("agent_run_invalid_explicit_reference"))?;
    let actual_hash = crate::cas::hash::content_hash_str(&full_content);
    if reference
        .content_hash
        .as_deref()
        .is_some_and(|expected| expected != actual_hash)
    {
        return Err(AppError::msg("agent_run_explicit_reference_changed"));
    }
    let (source_span_start, source_span_end, content) = if let Some(range) = &reference.utf8_range {
        if range.start > range.end
            || range.end > full_content.len()
            || !full_content.is_char_boundary(range.start)
            || !full_content.is_char_boundary(range.end)
        {
            return Err(AppError::msg("agent_run_invalid_explicit_reference"));
        }
        (
            range.start as i64,
            range.end as i64,
            full_content[range.start..range.end].to_string(),
        )
    } else {
        (0, full_content.len() as i64, full_content)
    };
    if content.chars().count() > MAX_EXPLICIT_MATERIAL_CHARS {
        return Err(AppError::msg("agent_run_context_too_large"));
    }
    Ok(RunContextMaterial {
        role: DomainMaterialRole::Reference,
        source_path: path.replace('\\', "/"),
        content_hash: actual_hash,
        source_span_start,
        source_span_end,
        content,
    })
}
