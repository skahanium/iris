//! Volatile, single-document classified Agent Runs.
//!
//! This module deliberately owns no database or CEF handle.  Prompt text,
//! decrypted document text and model output live only in zeroizing in-memory
//! values and are invalidated when their document context is cleared.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::ai_runtime::classified_document_policy_repository::load_classified_policy_decision_engine;
use crate::ai_runtime::policy_decision_engine::{CapabilityDecision, DocumentCapability};
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, AssistantRunEvent, AssistantRunGetResponse, AssistantRunSnapshot,
    AssistantSessionRef, RunEventPayload, RunEventType, RunState, SafeRunErrorCode, SecurityDomain,
};
use crate::crypto::classified_io;
use crate::crypto::vault_key::VAULT_KEY;
use crate::error::{AppError, AppResult};
use crate::storage::paths::{is_classified_note_path, resolve_vault_path};

const CONTEXT_TTL: Duration = Duration::from_secs(10 * 60);
const RUN_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_CLIENT_REQUEST_ID_CHARS: usize = 160;
const MAX_USER_MESSAGE_CHARS: usize = 16_000;

/// Opaque one-document context capability returned only for an unlocked editor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClassifiedDocumentContext {
    /// Opaque, short-lived capability. It is never a path.
    pub(crate) context_ref: String,
}

struct ContextEntry {
    path: String,
    created_at: Instant,
}

struct RunEntry {
    turn_id: String,
    session: AssistantSessionRef,
    context_ref: String,
    message: Zeroizing<String>,
    document: Zeroizing<String>,
    state: RunState,
    state_version: u64,
    events: Vec<AssistantRunEvent>,
    output: Option<Zeroizing<String>>,
    created_at: Instant,
}

/// Process-local storage for classified execution. Dropping or clearing this
/// store zeroizes all content-bearing fields.
#[derive(Default)]
pub(crate) struct ClassifiedEphemeralStore {
    contexts: HashMap<String, ContextEntry>,
    runs: HashMap<String, RunEntry>,
}

impl ClassifiedEphemeralStore {
    /// Forget every classified context and result immediately.
    pub(crate) fn clear(&mut self) {
        self.contexts.clear();
        self.runs.clear();
    }

    /// Mint a capability for exactly one currently opened classified document.
    pub(crate) fn open_context(
        &mut self,
        vault: &Path,
        path: &str,
    ) -> AppResult<ClassifiedDocumentContext> {
        self.prune();
        require_unlocked()?;
        if !is_classified_note_path(path) {
            return Err(AppError::msg(
                "agent_run_classified_context_not_current_document",
            ));
        }
        let resolved = resolve_vault_path(vault, path)?;
        if !resolved.is_file() {
            return Err(AppError::msg(
                "agent_run_classified_context_not_current_document",
            ));
        }
        let context_ref = uuid::Uuid::new_v4().to_string();
        self.contexts.insert(
            context_ref.clone(),
            ContextEntry {
                path: path.replace('\\', "/"),
                created_at: Instant::now(),
            },
        );
        Ok(ClassifiedDocumentContext { context_ref })
    }

    /// Admit a single explicit classified-document analysis without persistence.
    pub(crate) fn accept(
        &mut self,
        vault: &Path,
        client_request_id: &str,
        message: String,
        context_ref: &str,
    ) -> AppResult<AssistantRunAccepted> {
        self.prune();
        require_unlocked()?;
        if client_request_id.trim().is_empty()
            || client_request_id.chars().count() > MAX_CLIENT_REQUEST_ID_CHARS
            || message.trim().is_empty()
            || message.chars().count() > MAX_USER_MESSAGE_CHARS
        {
            return Err(AppError::msg("agent_run_invalid_request"));
        }
        let context = self
            .contexts
            .get(context_ref)
            .ok_or_else(|| AppError::msg("agent_run_classified_context_expired"))?;
        let policy = load_classified_policy_decision_engine(vault)?;
        let scope = policy.effective_document_scope(&context.path);
        if scope.decision_for(DocumentCapability::Read) == CapabilityDecision::Deny
            || scope.decision_for(DocumentCapability::SendToModel) == CapabilityDecision::Deny
        {
            return Err(AppError::msg("agent_run_permission_denied"));
        }
        let document = read_classified_document(vault, &context.path)?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let turn_id = uuid::Uuid::new_v4().to_string();
        let session = AssistantSessionRef {
            domain: SecurityDomain::Classified,
            session_key: format!("ephemeral-{client_request_id}"),
        };
        let accepted = AssistantRunEvent::new(
            run_id.clone(),
            1,
            0,
            RunEventType::Accepted,
            chrono::Utc::now().to_rfc3339(),
            RunEventPayload::Accepted {
                turn_id: turn_id.clone(),
                session_key: session.session_key.clone(),
            },
        )
        .map_err(AppError::msg)?;
        self.runs.insert(
            run_id.clone(),
            RunEntry {
                turn_id: turn_id.clone(),
                session: session.clone(),
                context_ref: context_ref.to_string(),
                message: Zeroizing::new(message),
                document: Zeroizing::new(document),
                state: RunState::Accepted,
                state_version: 0,
                events: vec![accepted],
                output: None,
                created_at: Instant::now(),
            },
        );
        Ok(AssistantRunAccepted {
            run_id,
            turn_id,
            session,
            state: RunState::Accepted,
            state_version: 0,
        })
    }

    /// Return redacted model input only while the bound context remains valid.
    pub(crate) fn prompt(&self, run_id: &str) -> AppResult<(String, String)> {
        let run = self.run(run_id)?;
        self.require_context(&run.context_ref)?;
        Ok((run.message.to_string(), run.document.to_string()))
    }

    /// Append a non-content lifecycle transition and return its safe event.
    pub(crate) fn transition(
        &mut self,
        run_id: &str,
        state: RunState,
        stage: &str,
    ) -> AppResult<AssistantRunEvent> {
        let run = self.run_mut(run_id)?;
        run.state = state;
        run.state_version += 1;
        let event = AssistantRunEvent::new(
            run_id,
            (run.events.len() + 1) as u64,
            run.state_version,
            RunEventType::StageChanged,
            chrono::Utc::now().to_rfc3339(),
            RunEventPayload::StageChanged {
                state,
                stage: stage.to_string(),
            },
        )
        .map_err(AppError::msg)?;
        run.events.push(event.clone());
        Ok(event)
    }

    /// Complete a Run without sending classified output over the event bus.
    pub(crate) fn complete(
        &mut self,
        run_id: &str,
        output: String,
    ) -> AppResult<AssistantRunEvent> {
        let run = self.run_mut(run_id)?;
        run.state = RunState::Completed;
        run.state_version += 1;
        run.output = Some(Zeroizing::new(output));
        let event = AssistantRunEvent::new(
            run_id,
            (run.events.len() + 1) as u64,
            run.state_version,
            RunEventType::Completed,
            chrono::Utc::now().to_rfc3339(),
            RunEventPayload::Completed { message_id: None },
        )
        .map_err(AppError::msg)?;
        run.events.push(event.clone());
        Ok(event)
    }

    /// Fail a Run with a safe code and no classified details.
    pub(crate) fn fail(
        &mut self,
        run_id: &str,
        code: SafeRunErrorCode,
    ) -> AppResult<AssistantRunEvent> {
        let run = self.run_mut(run_id)?;
        if run.state.is_terminal() {
            return run
                .events
                .last()
                .cloned()
                .ok_or_else(|| AppError::msg("agent_run_not_found"));
        }
        run.state = RunState::Failed;
        run.state_version += 1;
        let event = AssistantRunEvent::new(
            run_id,
            (run.events.len() + 1) as u64,
            run.state_version,
            RunEventType::Failed,
            chrono::Utc::now().to_rfc3339(),
            RunEventPayload::Failed {
                code,
                message: safe_message(code).to_string(),
            },
        )
        .map_err(AppError::msg)?;
        run.events.push(event.clone());
        Ok(event)
    }

    /// Cancel a Run and invalidate all content from it.
    pub(crate) fn cancel(&mut self, run_id: &str) -> AppResult<AssistantRunEvent> {
        let run = self.run_mut(run_id)?;
        run.state = RunState::Cancelled;
        run.state_version += 1;
        run.output = None;
        run.document = Zeroizing::new(String::new());
        run.message = Zeroizing::new(String::new());
        let event = AssistantRunEvent::new(
            run_id,
            (run.events.len() + 1) as u64,
            run.state_version,
            RunEventType::Cancelled,
            chrono::Utc::now().to_rfc3339(),
            RunEventPayload::Cancelled {
                reason: "user_cancelled".into(),
            },
        )
        .map_err(AppError::msg)?;
        run.events.push(event.clone());
        Ok(event)
    }

    /// Consume the final output exactly once for the same document context.
    pub(crate) fn take_result(&mut self, run_id: &str, context_ref: &str) -> AppResult<String> {
        self.require_context(context_ref)?;
        let run = self
            .runs
            .remove(run_id)
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
        if run.context_ref != context_ref || run.state != RunState::Completed {
            return Err(AppError::msg("agent_run_classified_context_expired"));
        }
        run.output
            .as_deref()
            .map(|value| value.to_string())
            .ok_or_else(|| AppError::msg("agent_run_classified_result_expired"))
    }

    /// Return only lifecycle metadata for an active transient Run.
    pub(crate) fn get(&self, run_id: &str) -> AppResult<Option<AssistantRunGetResponse>> {
        let Some(run) = self.runs.get(run_id) else {
            return Ok(None);
        };
        Ok(Some(AssistantRunGetResponse {
            run: AssistantRunSnapshot {
                run_id: run_id.to_string(),
                turn_id: run.turn_id.clone(),
                session: run.session.clone(),
                state: run.state,
                state_version: run.state_version,
                final_message_id: None,
                pending_confirmation: None,
                recovery: None,
            },
            events: run.events.clone(),
        }))
    }

    fn run(&self, run_id: &str) -> AppResult<&RunEntry> {
        self.runs
            .get(run_id)
            .ok_or_else(|| AppError::msg("agent_run_not_found"))
    }
    fn run_mut(&mut self, run_id: &str) -> AppResult<&mut RunEntry> {
        self.runs
            .get_mut(run_id)
            .ok_or_else(|| AppError::msg("agent_run_not_found"))
    }
    fn require_context(&self, context_ref: &str) -> AppResult<()> {
        let context = self
            .contexts
            .get(context_ref)
            .ok_or_else(|| AppError::msg("agent_run_classified_context_expired"))?;
        if context.created_at.elapsed() > CONTEXT_TTL {
            return Err(AppError::msg("agent_run_classified_context_expired"));
        }
        require_unlocked()
    }
    fn prune(&mut self) {
        self.contexts
            .retain(|_, entry| entry.created_at.elapsed() <= CONTEXT_TTL);
        self.runs
            .retain(|_, entry| entry.created_at.elapsed() <= RUN_TTL);
    }
}

fn require_unlocked() -> AppResult<()> {
    let key = VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("agent_run_classified_vault_locked"))?
        .read()
        .map_err(|_| AppError::msg("agent_run_classified_vault_locked"))?;
    if key.is_unlocked() {
        Ok(())
    } else {
        Err(AppError::msg("agent_run_classified_vault_locked"))
    }
}

fn read_classified_document(vault: &Path, path: &str) -> AppResult<String> {
    let raw = std::fs::read(resolve_vault_path(vault, path)?)?;
    if classified_io::has_csef_magic(&raw) {
        let key = VAULT_KEY
            .get()
            .ok_or_else(|| AppError::msg("agent_run_classified_vault_locked"))?
            .read()
            .map_err(|_| AppError::msg("agent_run_classified_vault_locked"))?;
        let plaintext = classified_io::decrypt_cef(&raw, key.key()?)?;
        String::from_utf8(plaintext).map_err(|_| AppError::msg("agent_run_invalid_request"))
    } else {
        String::from_utf8(raw).map_err(|_| AppError::msg("agent_run_invalid_request"))
    }
}

pub(crate) fn safe_message(code: SafeRunErrorCode) -> &'static str {
    match code {
        SafeRunErrorCode::PermissionDenied => "当前涉密文档未获授权读取或发送给模型。",
        SafeRunErrorCode::ClassifiedContextRequired => "请先明确附带当前打开的涉密文档。",
        SafeRunErrorCode::ClassifiedContextExpired => "当前涉密文档上下文已失效，请重新附带。",
        SafeRunErrorCode::ClassifiedVaultLocked => "涉密保险库已锁定，请解锁后重试。",
        SafeRunErrorCode::ProviderUnavailable => "模型服务暂时不可用，请稍后重试。",
        SafeRunErrorCode::ProviderTimeout => "模型服务响应超时，请稍后重试。",
        _ => "本次涉密分析无法完成，请重新附带当前文档后重试。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::vault_key::{init_vault_key, VAULT_KEY_TEST_LOCK};

    fn with_unlocked_key() -> std::sync::MutexGuard<'static, ()> {
        let guard = VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        init_vault_key();
        let mut key = VAULT_KEY
            .get()
            .expect("vault key initialized")
            .write()
            .expect("vault key write lock");
        key.set_test_key([41; 32]);
        drop(key);
        guard
    }

    #[test]
    fn completed_result_is_single_use_and_never_creates_cef_history() {
        let _key_guard = with_unlocked_key();
        let temp = tempfile::tempdir().expect("temp vault");
        let document = temp.path().join(".classified").join("current.md");
        std::fs::create_dir_all(document.parent().expect("parent")).expect("classified dir");
        std::fs::write(&document, "classified body").expect("fixture document");

        let mut store = ClassifiedEphemeralStore::default();
        let context = store
            .open_context(temp.path(), ".classified/current.md")
            .expect("context");
        let accepted = store
            .accept(
                temp.path(),
                "client-request",
                "analyze the current document".into(),
                &context.context_ref,
            )
            .expect("accepted");
        let (_, prompt_document) = store.prompt(&accepted.run_id).expect("prompt");
        assert_eq!(prompt_document, "classified body");
        store
            .complete(&accepted.run_id, "temporary answer".into())
            .expect("complete");
        assert_eq!(
            store
                .take_result(&accepted.run_id, &context.context_ref)
                .expect("one-time result"),
            "temporary answer"
        );
        assert!(store
            .take_result(&accepted.run_id, &context.context_ref)
            .is_err());
        assert!(!temp.path().join(".classified/.iris-ai").exists());
    }

    #[test]
    fn clearing_context_invalidates_pending_runs() {
        let _key_guard = with_unlocked_key();
        let temp = tempfile::tempdir().expect("temp vault");
        let document = temp.path().join(".classified").join("current.md");
        std::fs::create_dir_all(document.parent().expect("parent")).expect("classified dir");
        std::fs::write(&document, "classified body").expect("fixture document");
        let mut store = ClassifiedEphemeralStore::default();
        let context = store
            .open_context(temp.path(), ".classified/current.md")
            .expect("context");
        let accepted = store
            .accept(
                temp.path(),
                "request",
                "question".into(),
                &context.context_ref,
            )
            .expect("accepted");
        store.clear();
        assert!(store.prompt(&accepted.run_id).is_err());
    }

    #[test]
    fn rejects_empty_user_input_before_reading_or_dispatching() {
        let _key_guard = with_unlocked_key();
        let temp = tempfile::tempdir().expect("temp vault");
        let document = temp.path().join(".classified").join("current.md");
        std::fs::create_dir_all(document.parent().expect("parent")).expect("classified dir");
        std::fs::write(&document, "classified body").expect("fixture document");
        let mut store = ClassifiedEphemeralStore::default();
        let context = store
            .open_context(temp.path(), ".classified/current.md")
            .expect("context");

        let error = store
            .accept(temp.path(), "request", "   ".into(), &context.context_ref)
            .expect_err("empty input must be rejected");
        assert_eq!(error.to_string(), "agent_run_invalid_request");
    }
}
