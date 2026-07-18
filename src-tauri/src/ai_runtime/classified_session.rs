//! Encrypted classified AI thread storage.
//!
//! Thread data is persisted as CEF-encrypted JSON files under
//! `.classified/.iris-ai/sessions/<uuid>.cef`. A thread index is stored at
//! `.classified/.iris-ai/index.cef`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai_runtime::run_contract::RunEventPayload;
#[cfg(test)]
use crate::ai_runtime::run_contract::{
    transition_if_version, AssistantRunAccepted, AssistantRunEvent, AssistantRunGetResponse,
    AssistantRunSnapshot, AssistantSessionRef, RunEventType, RunState, SecurityDomain,
};
use crate::crypto::classified_io;
use crate::crypto::vault_key::{VaultKey, VAULT_KEY};
use crate::error::{AppError, AppResult};

const AI_DIR_NAME: &str = ".iris-ai";
const SESSIONS_DIR_NAME: &str = "sessions";
const INDEX_FILE_NAME: &str = "index.cef";
const CLASSIFIED_SESSION_SCHEMA_VERSION: u32 = 3;

/// A classified conversation persisted only in a CEF-encrypted file.
///
/// The schema deliberately has no document binding. Explicit references belong to
/// individual messages or Runs, so changing the active editor can never alter
/// conversation identity or the material available to a Run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiThread {
    pub version: u32,
    pub thread_id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub messages: Vec<ClassifiedAiMessage>,
    #[serde(default)]
    pub turns: Vec<ClassifiedAiTurn>,
    #[serde(default)]
    pub runs: Vec<ClassifiedAiRun>,
    #[serde(default)]
    pub events: Vec<ClassifiedAiRunEvent>,
    #[serde(default)]
    pub evidence: Vec<ClassifiedAiEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<serde_json::Value>,
}

/// A single classified conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiMessage {
    pub seq: i64,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_parts: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explicit_references: Vec<serde_json::Value>,
    pub created_at: String,
}

/// A logical user turn within a classified conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiTurn {
    pub turn_id: String,
    pub user_message_seq: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explicit_references: Vec<serde_json::Value>,
    pub created_at: String,
}

/// Durable Run summary for a classified conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiRun {
    pub run_id: String,
    pub turn_id: String,
    #[serde(default)]
    pub client_request_id: String,
    pub status: String,
    pub state_version: u64,
    pub effect: String,
    /// Explicit editor action scoped to this Run and retained only in CEF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_action: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_message_seq: Option<i64>,
    #[serde(default)]
    pub envelope: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Replayable, redacted event belonging to a classified Run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiRunEvent {
    pub run_id: String,
    pub seq: u64,
    pub state_version: u64,
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub created_at: String,
}

/// Classified evidence metadata, kept inside the same CEF boundary as its Run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedAiEvidence {
    pub evidence_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_run_id: Option<String>,
    pub source_kind: String,
    #[serde(default)]
    pub display_metadata: serde_json::Value,
    pub created_at: String,
}

/// Schema used only while migrating v1 CEF files. It is never written again.
#[derive(Debug, Clone, Deserialize)]
struct LegacyClassifiedAiThread {
    version: u32,
    thread_id: String,
    document_path: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    messages: Vec<ClassifiedAiMessage>,
    #[serde(default)]
    evidence_packets: Vec<serde_json::Value>,
    token_usage: Option<serde_json::Value>,
}

/// Summary of a classified conversation (for listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedAiThreadSummary {
    pub thread_id: String,
    pub title: String,
    pub message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// CEF-only acceptance facts for a classified Agent Run.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct ClassifiedRunAcceptInput {
    pub(crate) client_request_id: String,
    pub(crate) session_key: Option<String>,
    pub(crate) run_id: String,
    pub(crate) turn_id: String,
    pub(crate) message: String,
    pub(crate) content_parts: Option<serde_json::Value>,
    pub(crate) explicit_references: Vec<serde_json::Value>,
    pub(crate) explicit_action: Option<serde_json::Value>,
    pub(crate) envelope: serde_json::Value,
    pub(crate) effect: String,
}
/// In-memory CEF index cache, strictly scoped to one vault path.
#[derive(Debug, Clone)]
struct ClassifiedThreadIndexCache {
    vault: PathBuf,
    index: Vec<ClassifiedAiThreadSummary>,
}

static THREAD_INDEX_CACHE: RwLock<Option<ClassifiedThreadIndexCache>> = RwLock::new(None);

fn vault_key_read() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    VAULT_KEY
        .get()
        .ok_or_else(|| AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|e| AppError::msg(format!("VAULT_KEY lock error: {e}")))
}

fn require_unlocked() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    let vk = vault_key_read()?;
    if !vk.is_unlocked() {
        return Err(AppError::msg("保险库未解锁"));
    }
    Ok(vk)
}

/// Resolve the `.classified/.iris-ai` directory path.
fn ai_dir(vault: &Path) -> PathBuf {
    vault.join(".classified").join(AI_DIR_NAME)
}

/// Resolve the sessions subdirectory.
fn sessions_dir(vault: &Path) -> PathBuf {
    ai_dir(vault).join(SESSIONS_DIR_NAME)
}

/// Resolve the index file path.
fn index_path(vault: &Path) -> PathBuf {
    ai_dir(vault).join(INDEX_FILE_NAME)
}

/// Resolve the file path for a specific thread.
fn thread_file_path(vault: &Path, thread_id: &str) -> PathBuf {
    sessions_dir(vault).join(format!("{thread_id}.cef"))
}

fn validate_thread_id(thread_id: &str) -> AppResult<Uuid> {
    let parsed =
        Uuid::parse_str(thread_id).map_err(|_| AppError::msg("涉密 AI thread_id 必须是 UUID"))?;
    if parsed.to_string() != thread_id.to_ascii_lowercase() {
        return Err(AppError::msg("涉密 AI thread_id 必须使用标准 UUID 文件名"));
    }
    Ok(parsed)
}

/// Ensure the `.iris-ai/sessions` directory structure exists.
fn ensure_ai_dirs(vault: &Path) -> AppResult<()> {
    let dir = sessions_dir(vault);
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

/// Load and decrypt the conversation index from disk.
fn load_index(vault: &Path, key: &[u8; 32]) -> AppResult<Vec<ClassifiedAiThreadSummary>> {
    let path = index_path(vault);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read(&path)?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let plaintext = classified_io::decrypt_cef(&raw, key)?;
    Ok(serde_json::from_slice(&plaintext)?)
}

/// Encrypt and save the conversation index. The index intentionally carries no
/// document path, because a classified conversation is not editor-owned state.
fn save_index(vault: &Path, key: &[u8; 32], index: &[ClassifiedAiThreadSummary]) -> AppResult<()> {
    ensure_ai_dirs(vault)?;
    let plaintext = serde_json::to_vec(index)?;
    let encrypted = classified_io::encrypt_cef(&plaintext, key)?;
    fs::write(index_path(vault), &encrypted)?;
    Ok(())
}

/// Update the in-memory cache after a mutation, retaining the owning vault identity.
fn update_cache(vault: &Path, index: &[ClassifiedAiThreadSummary]) {
    if let Ok(mut cache) = THREAD_INDEX_CACHE.write() {
        *cache = Some(ClassifiedThreadIndexCache {
            vault: vault.to_path_buf(),
            index: index.to_vec(),
        });
    }
}

/// List all classified conversations. Filtering by current document is not
/// supported: explicit references are a message/Run property, never a session
/// identity.
pub fn classified_ai_thread_list(vault: &Path) -> AppResult<Vec<ClassifiedAiThreadSummary>> {
    let vk = require_unlocked()?;
    let key = vk.key()?;

    if let Ok(cache) = THREAD_INDEX_CACHE.read() {
        if let Some(cached) = cache.as_ref().filter(|cached| cached.vault == vault) {
            return Ok(cached.index.clone());
        }
    }

    let index = load_index(vault, key)?;
    update_cache(vault, &index);
    Ok(index)
}

/// Build deterministic historical turn/run/event records for a conversation
/// that predates the unified Run lifecycle.
fn derive_legacy_lifecycle(
    messages: &mut [ClassifiedAiMessage],
    created_at: &str,
) -> AppResult<(
    Vec<ClassifiedAiTurn>,
    Vec<ClassifiedAiRun>,
    Vec<ClassifiedAiRunEvent>,
)> {
    let mut turns = Vec::new();
    let mut runs = Vec::new();
    let mut events = Vec::new();
    let mut active_turn_id = None;
    let mut active_run_id = None;

    for message in messages {
        if message.role == "user" {
            let turn_id = Uuid::new_v4().to_string();
            turns.push(ClassifiedAiTurn {
                turn_id: turn_id.clone(),
                user_message_seq: message.seq,
                explicit_references: std::mem::take(&mut message.explicit_references),
                created_at: message.created_at.clone(),
            });
            let run_id = Uuid::new_v4().to_string();
            runs.push(ClassifiedAiRun {
                run_id: run_id.clone(),
                turn_id: turn_id.clone(),
                client_request_id: String::new(),
                status: "completed".into(),
                state_version: 1,
                effect: "answer".into(),
                explicit_action: None,
                final_message_seq: None,
                envelope: serde_json::Value::Null,
                created_at: message.created_at.clone(),
                updated_at: created_at.to_string(),
            });
            events.push(ClassifiedAiRunEvent {
                run_id: run_id.clone(),
                seq: 1,
                state_version: 1,
                event_type: "completed".into(),
                payload: serde_json::to_value(RunEventPayload::Completed { message_id: None })?,
                created_at: created_at.to_string(),
            });
            active_turn_id = Some(turn_id);
            active_run_id = Some(run_id);
        } else if message.role == "assistant" {
            if let Some(run_id) = active_run_id.as_deref() {
                let final_message_id = classified_final_message_id(run_id, message.seq);
                if let Some(run) = runs.iter_mut().find(|run| run.run_id == run_id) {
                    run.final_message_seq = Some(message.seq);
                }
                if let Some(event) = events.iter_mut().find(|event| event.run_id == run_id) {
                    event.payload = serde_json::to_value(RunEventPayload::Completed {
                        message_id: Some(final_message_id),
                    })?;
                }
            }
        }
        message.turn_id = active_turn_id.clone();
    }

    Ok((turns, runs, events))
}

/// Transform a v1 document-bound thread into the unbound conversation schema.
fn migrate_legacy_thread(legacy: LegacyClassifiedAiThread) -> AppResult<ClassifiedAiThread> {
    validate_thread_id(&legacy.thread_id)?;
    if legacy.version != 1 {
        return Err(AppError::msg("invalid legacy classified session schema"));
    }
    if legacy.document_path.trim().is_empty() {
        return Err(AppError::msg("invalid legacy classified session schema"));
    }

    let mut messages = legacy.messages;
    let (turns, runs, events) = derive_legacy_lifecycle(&mut messages, &legacy.updated_at)?;
    let origin_run_id = runs.last().map(|run| run.run_id.clone());
    let evidence = legacy
        .evidence_packets
        .into_iter()
        .map(|packet| ClassifiedAiEvidence {
            evidence_id: Uuid::new_v4().to_string(),
            origin_run_id: origin_run_id.clone(),
            source_kind: "legacy_packet".into(),
            display_metadata: packet,
            created_at: legacy.updated_at.clone(),
        })
        .collect();

    Ok(ClassifiedAiThread {
        version: CLASSIFIED_SESSION_SCHEMA_VERSION,
        thread_id: legacy.thread_id,
        title: legacy.title,
        created_at: legacy.created_at,
        updated_at: legacy.updated_at,
        messages,
        turns,
        runs,
        events,
        evidence,
        token_usage: legacy.token_usage,
    })
}

/// Upgrade a v2 unbound conversation to the final v3 lifecycle schema.
///
/// v2 already removed document binding, but it could still contain records
/// written before Runs, Turns, and Events became mandatory. The transformation
/// is idempotent and is persisted by the load path through an atomic CEF swap.
fn migrate_v2_thread(mut thread: ClassifiedAiThread) -> AppResult<ClassifiedAiThread> {
    if thread.version != 2 {
        return Err(AppError::msg("invalid classified session schema"));
    }
    thread.version = CLASSIFIED_SESSION_SCHEMA_VERSION;
    normalize_thread_for_write(thread)
}

/// Normalize client-created conversations so only the final v3 schema can be written.
fn normalize_thread_for_write(mut thread: ClassifiedAiThread) -> AppResult<ClassifiedAiThread> {
    validate_thread_id(&thread.thread_id)?;
    thread.version = CLASSIFIED_SESSION_SCHEMA_VERSION;
    if thread.turns.is_empty() && !thread.messages.is_empty() {
        let (turns, runs, events) =
            derive_legacy_lifecycle(&mut thread.messages, &thread.updated_at)?;
        thread.turns = turns;
        thread.runs = runs;
        thread.events = events;
    }
    Ok(thread)
}

/// Encrypt, validate, then atomically replace a CEF conversation file.
///
/// Any serialization, encryption, or validation error happens before the final
/// rename, so callers retain the original readable file on migration failure.
fn write_thread_atomically(
    path: &Path,
    thread: &ClassifiedAiThread,
    key: &[u8; 32],
) -> AppResult<()> {
    validate_thread_id(&thread.thread_id)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("invalid classified session path"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::msg("invalid classified session path"))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));

    let write_result = (|| -> AppResult<()> {
        let plaintext = serde_json::to_vec(thread)?;
        let encrypted = classified_io::encrypt_cef(&plaintext, key)?;
        fs::write(&temporary, encrypted)?;

        let verification_raw = fs::read(&temporary)?;
        let verification_plaintext = classified_io::decrypt_cef(&verification_raw, key)?;
        let verified: ClassifiedAiThread = serde_json::from_slice(&verification_plaintext)?;
        validate_thread_id(&verified.thread_id)?;
        if verified.version != CLASSIFIED_SESSION_SCHEMA_VERSION {
            return Err(AppError::msg("invalid classified session schema"));
        }

        fs::rename(&temporary, path)?;
        Ok(())
    })();

    if write_result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

/// Load and decrypt a classified conversation by id, lazily migrating a legacy
/// document-bound CEF file only after the transformed temporary CEF validates.
pub fn classified_ai_thread_load(vault: &Path, thread_id: String) -> AppResult<ClassifiedAiThread> {
    let vk = require_unlocked()?;
    let key = vk.key()?;
    validate_thread_id(&thread_id)?;

    let path = thread_file_path(vault, &thread_id);
    if !path.exists() {
        return Err(AppError::msg("classified conversation not found"));
    }

    let raw = fs::read(&path)?;
    let plaintext = classified_io::decrypt_cef(&raw, key)?;
    let document: serde_json::Value = serde_json::from_slice(&plaintext)?;
    let version = document
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as u32;

    let thread = match version {
        1 => {
            let legacy: LegacyClassifiedAiThread = serde_json::from_value(document)?;
            let migrated = migrate_legacy_thread(legacy)?;
            write_thread_atomically(&path, &migrated, key)?;
            migrated
        }
        2 => {
            let legacy: ClassifiedAiThread = serde_json::from_value(document)?;
            let migrated = migrate_v2_thread(legacy)?;
            write_thread_atomically(&path, &migrated, key)?;
            migrated
        }
        CLASSIFIED_SESSION_SCHEMA_VERSION => {
            let thread: ClassifiedAiThread = serde_json::from_value(document.clone())?;
            let normalized = normalize_thread_for_write(thread)?;
            if serde_json::to_value(&normalized)? != document {
                write_thread_atomically(&path, &normalized, key)?;
            }
            normalized
        }
        _ => return Err(AppError::msg("unsupported classified session schema")),
    };

    if thread.thread_id != thread_id {
        return Err(AppError::msg("classified conversation identifier mismatch"));
    }
    Ok(thread)
}

/// Accept a classified Run into CEF storage only, before any provider or tool work.
#[cfg(test)]
pub(crate) fn classified_run_accept(
    vault: &Path,
    input: ClassifiedRunAcceptInput,
) -> AppResult<AssistantRunAccepted> {
    validate_thread_id(&input.run_id)?;
    validate_thread_id(&input.turn_id)?;
    if input.client_request_id.trim().is_empty() || input.message.trim().is_empty() {
        return Err(AppError::msg("agent_run_invalid_request"));
    }

    if let Some(existing) = find_classified_run_by_client_request(vault, &input.client_request_id)?
    {
        return Ok(existing);
    }

    let mut thread = match input.session_key.as_deref() {
        Some(session_key) => classified_ai_thread_load(vault, session_key.to_string())?,
        None => new_classified_thread(),
    };
    let now = chrono::Utc::now().to_rfc3339();
    let message_seq = thread.messages.last().map_or(1, |message| message.seq + 1);
    thread.messages.push(ClassifiedAiMessage {
        seq: message_seq,
        role: "user".into(),
        content: input.message,
        turn_id: Some(input.turn_id.clone()),
        content_parts: input.content_parts,
        tool_calls: None,
        explicit_references: input.explicit_references.clone(),
        created_at: now.clone(),
    });
    thread.turns.push(ClassifiedAiTurn {
        turn_id: input.turn_id.clone(),
        user_message_seq: message_seq,
        explicit_references: input.explicit_references,
        created_at: now.clone(),
    });
    thread.runs.push(ClassifiedAiRun {
        run_id: input.run_id.clone(),
        turn_id: input.turn_id.clone(),
        client_request_id: input.client_request_id.clone(),
        status: "accepted".into(),
        state_version: 0,
        effect: input.effect,
        explicit_action: input.explicit_action,
        final_message_seq: None,
        envelope: input.envelope,
        created_at: now.clone(),
        updated_at: now.clone(),
    });
    thread.events.push(ClassifiedAiRunEvent {
        run_id: input.run_id.clone(),
        seq: 1,
        state_version: 0,
        event_type: "accepted".into(),
        payload: serde_json::to_value(RunEventPayload::Accepted {
            turn_id: input.turn_id.clone(),
            session_key: thread.thread_id.clone(),
        })?,
        created_at: now.clone(),
    });
    thread.updated_at = now;
    let session_key = thread.thread_id.clone();
    classified_ai_thread_save(vault, thread)?;

    Ok(AssistantRunAccepted {
        client_request_id: input.client_request_id,
        run_id: input.run_id,
        turn_id: input.turn_id,
        session: AssistantSessionRef {
            domain: SecurityDomain::Classified,
            session_key,
        },
        state: RunState::Accepted,
        state_version: 0,
    })
}

#[cfg(test)]
fn new_classified_thread() -> ClassifiedAiThread {
    let now = chrono::Utc::now().to_rfc3339();
    ClassifiedAiThread {
        version: CLASSIFIED_SESSION_SCHEMA_VERSION,
        thread_id: Uuid::new_v4().to_string(),
        title: None,
        created_at: now.clone(),
        updated_at: now,
        messages: Vec::new(),
        turns: Vec::new(),
        runs: Vec::new(),
        events: Vec::new(),
        evidence: Vec::new(),
        token_usage: None,
    }
}

#[cfg(test)]
fn find_classified_run_by_client_request(
    vault: &Path,
    client_request_id: &str,
) -> AppResult<Option<AssistantRunAccepted>> {
    for summary in classified_ai_thread_list(vault)? {
        let thread = classified_ai_thread_load(vault, summary.thread_id.clone())?;
        if let Some(run) = thread
            .runs
            .iter()
            .find(|run| run.client_request_id == client_request_id)
        {
            let state = serde_json::from_value(serde_json::Value::String(run.status.clone()))
                .map_err(|_| AppError::msg("invalid classified run state"))?;
            return Ok(Some(AssistantRunAccepted {
                client_request_id: run.client_request_id.clone(),
                run_id: run.run_id.clone(),
                turn_id: run.turn_id.clone(),
                session: AssistantSessionRef {
                    domain: SecurityDomain::Classified,
                    session_key: thread.thread_id,
                },
                state,
                state_version: run.state_version,
            }));
        }
    }
    Ok(None)
}
/// Replay one classified Run from its CEF conversation without touching SQLite.
#[cfg(test)]
pub(crate) fn classified_run_get(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
) -> AppResult<Option<AssistantRunGetResponse>> {
    if session.domain != SecurityDomain::Classified {
        return Err(AppError::msg("agent_run_session_not_found"));
    }
    let thread = classified_ai_thread_load(vault, session.session_key.clone())?;
    let Some(run) = thread.runs.iter().find(|run| run.run_id == run_id) else {
        return Ok(None);
    };
    let state = run_state_from_wire(&run.status)?;
    let events = thread
        .events
        .iter()
        .filter(|event| event.run_id == run_id)
        .map(classified_event_to_run_event)
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Some(AssistantRunGetResponse {
        run: AssistantRunSnapshot {
            run_id: run.run_id.clone(),
            turn_id: run.turn_id.clone(),
            session: AssistantSessionRef {
                domain: SecurityDomain::Classified,
                session_key: thread.thread_id,
            },
            state,
            state_version: run.state_version,
            final_message_id: run
                .final_message_seq
                .map(|seq| classified_final_message_id(&run.run_id, seq)),
            pending_confirmation: None,
            recovery: None,
        },
        events,
    }))
}

/// Persist the preparing stage for a classified Run inside its CEF conversation.
#[cfg(test)]
pub(crate) fn classified_run_mark_preparing(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
) -> AppResult<AssistantRunEvent> {
    classified_run_transition(
        vault,
        session,
        run_id,
        0,
        RunState::Preparing,
        RunEventType::StageChanged,
        RunEventPayload::StageChanged {
            state: RunState::Preparing,
            stage: "正在准备".into(),
        },
    )
}

/// Persist the running stage for a classified Run inside its CEF conversation.
#[cfg(test)]
pub(crate) fn classified_run_mark_running(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    expected_state_version: u64,
) -> AppResult<AssistantRunEvent> {
    classified_run_transition(
        vault,
        session,
        run_id,
        expected_state_version,
        RunState::Running,
        RunEventType::StageChanged,
        RunEventPayload::StageChanged {
            state: RunState::Running,
            stage: "正在生成答复".into(),
        },
    )
}

/// Persist a classified final assistant message and its completed event together.
///
/// The CEF write is atomic. A cancellation that wins the optimistic version race
/// prevents this function from adding either an assistant message or completed event.
#[cfg(test)]
pub(crate) fn classified_run_complete(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    expected_state_version: u64,
    content: String,
) -> AppResult<AssistantRunEvent> {
    if session.domain != SecurityDomain::Classified || content.trim().is_empty() {
        return Err(AppError::msg("agent_run_invalid_request"));
    }
    let mut thread = classified_ai_thread_load(vault, session.session_key.clone())?;
    let run_index = find_classified_run_index(&thread, run_id)?;
    let current = run_state_from_wire(&thread.runs[run_index].status)?;
    let next = transition_if_version(
        current,
        thread.runs[run_index].state_version,
        expected_state_version,
        RunState::Completed,
    )
    .map_err(|error| AppError::msg(error.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    let message_seq = thread.messages.last().map_or(1, |message| message.seq + 1);
    let message_id = classified_final_message_id(run_id, message_seq);
    let turn_id = thread.runs[run_index].turn_id.clone();
    thread.messages.push(ClassifiedAiMessage {
        seq: message_seq,
        role: "assistant".into(),
        content,
        turn_id: Some(turn_id),
        content_parts: None,
        tool_calls: None,
        explicit_references: Vec::new(),
        created_at: now.clone(),
    });
    thread.runs[run_index].status = run_state_wire(RunState::Completed)?;
    thread.runs[run_index].state_version = next.state_version;
    thread.runs[run_index].final_message_seq = Some(message_seq);
    thread.runs[run_index].updated_at = now.clone();
    let event = append_classified_run_event(
        &mut thread,
        run_id,
        next.state_version,
        RunEventType::Completed,
        RunEventPayload::Completed {
            message_id: Some(message_id),
        },
        now,
    )?;
    classified_ai_thread_save(vault, thread)?;
    Ok(event)
}

/// Persist a safe classified failure unless cancellation has already won the race.
#[cfg(test)]
pub(crate) fn classified_run_fail(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    expected_state_version: u64,
    code: crate::ai_runtime::run_contract::SafeRunErrorCode,
) -> AppResult<Option<AssistantRunEvent>> {
    if session.domain != SecurityDomain::Classified {
        return Err(AppError::msg("agent_run_session_not_found"));
    }
    let mut thread = classified_ai_thread_load(vault, session.session_key.clone())?;
    let run_index = find_classified_run_index(&thread, run_id)?;
    let current = run_state_from_wire(&thread.runs[run_index].status)?;
    if current == RunState::Cancelled {
        return Ok(None);
    }
    let next = transition_if_version(
        current,
        thread.runs[run_index].state_version,
        expected_state_version,
        RunState::Failed,
    )
    .map_err(|error| AppError::msg(error.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    thread.runs[run_index].status = run_state_wire(RunState::Failed)?;
    thread.runs[run_index].state_version = next.state_version;
    thread.runs[run_index].updated_at = now.clone();
    let event = append_classified_run_event(
        &mut thread,
        run_id,
        next.state_version,
        RunEventType::Failed,
        RunEventPayload::Failed {
            code,
            message: classified_safe_failure_message(code).into(),
        },
        now,
    )?;
    classified_ai_thread_save(vault, thread)?;
    Ok(Some(event))
}

/// Terminalize an unfinished classified Run after an unexpected background exit.
///
/// The returned events are already durably stored and must be emitted in order by
/// the caller. Waiting-for-confirmation and paused Runs deliberately remain
/// untouched: they are not live provider work and may be recovered safely later.
#[cfg(test)]
pub(crate) fn classified_run_fail_unfinished(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    code: crate::ai_runtime::run_contract::SafeRunErrorCode,
) -> AppResult<Vec<AssistantRunEvent>> {
    let snapshot = classified_run_get(vault, session, run_id)?
        .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    if snapshot.run.state.is_terminal()
        || matches!(
            snapshot.run.state,
            RunState::AwaitingConfirmation | RunState::Paused
        )
    {
        return Ok(Vec::new());
    }

    let mut events = Vec::new();
    let expected_state_version = if snapshot.run.state == RunState::Accepted {
        let preparing = classified_run_mark_preparing(vault, session, run_id)?;
        let version = preparing.state_version();
        events.push(preparing);
        version
    } else {
        snapshot.run.state_version
    };
    if let Some(failed) = classified_run_fail(vault, session, run_id, expected_state_version, code)?
    {
        events.push(failed);
    }
    Ok(events)
}

#[cfg(test)]
fn classified_run_transition(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    expected_state_version: u64,
    target_state: RunState,
    event_type: RunEventType,
    payload: RunEventPayload,
) -> AppResult<AssistantRunEvent> {
    if session.domain != SecurityDomain::Classified {
        return Err(AppError::msg("agent_run_session_not_found"));
    }
    let mut thread = classified_ai_thread_load(vault, session.session_key.clone())?;
    let run_index = find_classified_run_index(&thread, run_id)?;
    let current = run_state_from_wire(&thread.runs[run_index].status)?;
    let next = transition_if_version(
        current,
        thread.runs[run_index].state_version,
        expected_state_version,
        target_state,
    )
    .map_err(|error| AppError::msg(error.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    thread.runs[run_index].status = run_state_wire(target_state)?;
    thread.runs[run_index].state_version = next.state_version;
    thread.runs[run_index].updated_at = now.clone();
    let event = append_classified_run_event(
        &mut thread,
        run_id,
        next.state_version,
        event_type,
        payload,
        now,
    )?;
    classified_ai_thread_save(vault, thread)?;
    Ok(event)
}

#[cfg(test)]
fn find_classified_run_index(thread: &ClassifiedAiThread, run_id: &str) -> AppResult<usize> {
    thread
        .runs
        .iter()
        .position(|run| run.run_id == run_id)
        .ok_or_else(|| AppError::msg("agent_run_not_found"))
}

#[cfg(test)]
fn append_classified_run_event(
    thread: &mut ClassifiedAiThread,
    run_id: &str,
    state_version: u64,
    event_type: RunEventType,
    payload: RunEventPayload,
    created_at: String,
) -> AppResult<AssistantRunEvent> {
    let seq = thread
        .events
        .iter()
        .filter(|event| event.run_id == run_id)
        .map(|event| event.seq)
        .max()
        .unwrap_or(0)
        + 1;
    let event = AssistantRunEvent::new(
        run_id,
        seq,
        state_version,
        event_type,
        created_at.clone(),
        payload.clone(),
    )
    .map_err(AppError::msg)?;
    let event_type = serde_json::to_value(event_type)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::msg("invalid classified run event"))?;
    thread.updated_at = created_at.clone();
    thread.events.push(ClassifiedAiRunEvent {
        run_id: run_id.to_string(),
        seq,
        state_version,
        event_type,
        payload: serde_json::to_value(payload)?,
        created_at,
    });
    Ok(event)
}

fn classified_final_message_id(run_id: &str, message_seq: i64) -> String {
    format!("{run_id}:message:{message_seq}")
}

#[cfg(test)]
fn classified_safe_failure_message(
    code: crate::ai_runtime::run_contract::SafeRunErrorCode,
) -> &'static str {
    match code {
        crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderUnavailable => {
            "模型服务暂时不可用，请稍后重试"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::ProviderTimeout => {
            "模型服务响应超时，请稍后重试"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::InvalidRequest => {
            "请求无法按当前运行能力处理"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::PermissionDenied => {
            "当前涉密文档未获授权读取或发送给模型"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::ClassifiedContextRequired => {
            "请先明确附带当前打开的涉密文档"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::ClassifiedContextExpired => {
            "当前涉密文档上下文已失效，请重新附带"
        }
        crate::ai_runtime::run_contract::SafeRunErrorCode::ClassifiedVaultLocked => {
            "涉密保险库已锁定，请解锁后重试"
        }
        _ => "运行暂时无法完成，请稍后重试",
    }
}
/// Cancel a classified Run by appending the same durable lifecycle event shape
/// used by normal-domain Runs, but solely inside the CEF conversation.
#[cfg(test)]
pub(crate) fn classified_run_cancel(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    expected_state_version: u64,
) -> AppResult<Option<AssistantRunEvent>> {
    if session.domain != SecurityDomain::Classified {
        return Err(AppError::msg("agent_run_session_not_found"));
    }
    let mut thread = classified_ai_thread_load(vault, session.session_key.clone())?;
    let run_index = thread
        .runs
        .iter()
        .position(|run| run.run_id == run_id)
        .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    let current = run_state_from_wire(&thread.runs[run_index].status)?;
    if current == RunState::Cancelled {
        return Ok(None);
    }
    let next = transition_if_version(
        current,
        thread.runs[run_index].state_version,
        expected_state_version,
        RunState::Cancelled,
    )
    .map_err(|error| AppError::msg(error.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    let event_seq = thread
        .events
        .iter()
        .filter(|event| event.run_id == run_id)
        .map(|event| event.seq)
        .max()
        .unwrap_or(0)
        + 1;
    let event = AssistantRunEvent::new(
        run_id,
        event_seq,
        next.state_version,
        RunEventType::Cancelled,
        now.clone(),
        RunEventPayload::Cancelled {
            reason: "用户取消运行".into(),
        },
    )
    .map_err(AppError::msg)?;
    thread.runs[run_index].status = run_state_wire(RunState::Cancelled)?;
    thread.runs[run_index].state_version = next.state_version;
    thread.runs[run_index].updated_at = now.clone();
    thread.events.push(ClassifiedAiRunEvent {
        run_id: run_id.to_string(),
        seq: event_seq,
        state_version: next.state_version,
        event_type: "cancelled".into(),
        payload: serde_json::to_value(RunEventPayload::Cancelled {
            reason: "用户取消运行".into(),
        })?,
        created_at: now.clone(),
    });
    thread.updated_at = now;
    classified_ai_thread_save(vault, thread)?;
    Ok(Some(event))
}

#[cfg(test)]
fn classified_event_to_run_event(event: &ClassifiedAiRunEvent) -> AppResult<AssistantRunEvent> {
    AssistantRunEvent::new(
        event.run_id.clone(),
        event.seq,
        event.state_version,
        serde_json::from_value(serde_json::Value::String(event.event_type.clone()))
            .map_err(|_| AppError::msg("invalid classified run event"))?,
        event.created_at.clone(),
        serde_json::from_value(event.payload.clone())
            .map_err(|_| AppError::msg("invalid classified run event"))?,
    )
    .map_err(AppError::msg)
}

#[cfg(test)]
fn run_state_from_wire(value: &str) -> AppResult<RunState> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| AppError::msg("invalid classified run state"))
}

#[cfg(test)]
fn run_state_wire(state: RunState) -> AppResult<String> {
    serde_json::to_value(state)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::msg("invalid classified run state"))
}
/// Retract one suffix while preserving the Conversation → Turn → Run graph.
fn retract_thread(thread: &mut ClassifiedAiThread, from_seq: i64) -> AppResult<u32> {
    if from_seq <= 0 {
        return Err(AppError::msg(
            "classified session sequence must be positive",
        ));
    }

    let removed_message_count = thread
        .messages
        .iter()
        .filter(|message| message.seq >= from_seq)
        .count();
    let mut removed_turn_ids = thread
        .messages
        .iter()
        .filter(|message| message.seq >= from_seq)
        .filter_map(|message| message.turn_id.clone())
        .collect::<std::collections::HashSet<_>>();
    for turn in &thread.turns {
        if turn.user_message_seq >= from_seq {
            removed_turn_ids.insert(turn.turn_id.clone());
        }
    }
    let removed_run_ids = thread
        .runs
        .iter()
        .filter(|run| removed_turn_ids.contains(&run.turn_id))
        .map(|run| run.run_id.clone())
        .collect::<std::collections::HashSet<_>>();

    thread.messages.retain(|message| message.seq < from_seq);
    thread
        .turns
        .retain(|turn| !removed_turn_ids.contains(&turn.turn_id));
    thread
        .runs
        .retain(|run| !removed_run_ids.contains(&run.run_id));
    thread
        .events
        .retain(|event| !removed_run_ids.contains(&event.run_id));
    thread.evidence.retain(|item| {
        item.origin_run_id
            .as_ref()
            .is_none_or(|run_id| !removed_run_ids.contains(run_id))
    });
    if removed_message_count > 0 {
        thread.updated_at = chrono::Utc::now().to_rfc3339();
    }
    Ok(removed_message_count as u32)
}

/// Rename a classified conversation without leaving the CEF boundary.
pub fn classified_ai_thread_rename(
    vault: &Path,
    thread_id: String,
    title: String,
) -> AppResult<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::msg("classified session title cannot be empty"));
    }
    let mut thread = classified_ai_thread_load(vault, thread_id)?;
    thread.title = Some(title.to_string());
    thread.updated_at = chrono::Utc::now().to_rfc3339();
    classified_ai_thread_save(vault, thread)
}

/// Retract a classified conversation suffix without creating a normal-domain mirror.
pub fn classified_ai_thread_retract(
    vault: &Path,
    thread_id: String,
    from_seq: i64,
) -> AppResult<u32> {
    let mut thread = classified_ai_thread_load(vault, thread_id)?;
    let deleted = retract_thread(&mut thread, from_seq)?;
    if deleted > 0 {
        classified_ai_thread_save(vault, thread)?;
    }
    Ok(deleted)
}
/// Encrypt and save a classified conversation in the v2 schema.
pub fn classified_ai_thread_save(vault: &Path, thread: ClassifiedAiThread) -> AppResult<()> {
    let vk = require_unlocked()?;
    let key = vk.key()?;
    let thread = normalize_thread_for_write(thread)?;

    ensure_ai_dirs(vault)?;
    let path = thread_file_path(vault, &thread.thread_id);
    write_thread_atomically(&path, &thread, key)?;

    let mut index = load_index(vault, key)?;
    let summary = ClassifiedAiThreadSummary {
        thread_id: thread.thread_id.clone(),
        title: thread
            .title
            .clone()
            .unwrap_or_else(|| derive_thread_title(&thread)),
        message_count: thread.messages.len() as u32,
        created_at: thread.created_at.clone(),
        updated_at: thread.updated_at.clone(),
    };

    if let Some(existing) = index
        .iter_mut()
        .find(|item| item.thread_id == thread.thread_id)
    {
        *existing = summary;
    } else {
        index.push(summary);
    }
    index.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    save_index(vault, key, &index)?;
    update_cache(vault, &index);
    Ok(())
}
/// Delete a classified AI thread.
pub fn classified_ai_thread_delete(vault: &Path, thread_id: String) -> AppResult<()> {
    let _vk = require_unlocked()?;
    let key = _vk.key()?;
    validate_thread_id(&thread_id)?;

    // Remove thread file
    let path = thread_file_path(vault, &thread_id);
    if path.exists() {
        fs::remove_file(&path)?;
    }

    // Remove from index
    let mut index = load_index(vault, key)?;
    index.retain(|t| t.thread_id != thread_id);
    save_index(vault, key, &index)?;
    update_cache(vault, &index);

    Ok(())
}

/// Clear the in-memory thread index cache.
pub fn classified_ai_cache_clear() -> AppResult<()> {
    if let Ok(mut cache) = THREAD_INDEX_CACHE.write() {
        *cache = None;
    }
    crate::ai_runtime::classified_retrieval::clear_classified_index();
    Ok(())
}

/// Derive a display title from the first user message.
fn derive_thread_title(thread: &ClassifiedAiThread) -> String {
    let first_user = thread
        .messages
        .iter()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("新对话");
    let trimmed = first_user.trim();
    if trimmed.is_empty() {
        return "新对话".to_string();
    }
    let chars: String = trimmed.chars().take(40).collect();
    if trimmed.chars().count() > 40 {
        format!("{chars}…")
    } else {
        chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = index as u8;
        }
        key
    }

    fn message(seq: i64, role: &str, content: &str) -> ClassifiedAiMessage {
        ClassifiedAiMessage {
            seq,
            role: role.into(),
            content: content.into(),
            turn_id: None,
            content_parts: None,
            tool_calls: None,
            explicit_references: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn thread(messages: Vec<ClassifiedAiMessage>) -> ClassifiedAiThread {
        ClassifiedAiThread {
            version: CLASSIFIED_SESSION_SCHEMA_VERSION,
            thread_id: "019f0871-0000-7000-8000-000000000001".into(),
            title: Some("测试会话".into()),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            messages,
            turns: vec![],
            runs: vec![],
            events: vec![],
            evidence: vec![],
            token_usage: None,
        }
    }

    #[test]
    fn thread_roundtrip_encrypt_decrypt() {
        let key = test_key();
        let thread = thread(vec![message(1, "user", "你好")]);
        let plaintext = serde_json::to_vec(&thread).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&plaintext)
            .unwrap()
            .get("document_path")
            .is_none());
        let encrypted = classified_io::encrypt_cef(&plaintext, &key).unwrap();
        assert!(classified_io::has_csef_magic(&encrypted));

        let decrypted = classified_io::decrypt_cef(&encrypted, &key).unwrap();
        let restored: ClassifiedAiThread = serde_json::from_slice(&decrypted).unwrap();
        assert_eq!(restored.thread_id, "019f0871-0000-7000-8000-000000000001");
        assert_eq!(restored.messages[0].content, "你好");
    }

    #[test]
    fn thread_summary_serialization_camel_case_without_document_path() {
        let summary = ClassifiedAiThreadSummary {
            thread_id: "abc".into(),
            title: "标题".into(),
            message_count: 5,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("threadId").is_some());
        assert!(json.get("messageCount").is_some());
        assert!(json.get("documentPath").is_none());
    }

    #[test]
    fn validate_thread_id_rejects_path_traversal() {
        assert!(validate_thread_id("../secret").is_err());
        assert!(validate_thread_id(".classified/secret").is_err());
        assert!(validate_thread_id("not-a-uuid").is_err());
        assert!(validate_thread_id("019f0871-0000-7000-8000-000000000002").is_ok());
    }

    #[test]
    fn derive_thread_title_from_first_user_message() {
        let title = derive_thread_title(&thread(vec![
            message(1, "user", "请帮我分析这份报告的关键数据点"),
            message(2, "assistant", "好的，我来分析"),
        ]));
        assert_eq!(title, "请帮我分析这份报告的关键数据点");
    }

    #[test]
    fn derive_thread_title_truncates_long_messages() {
        let long_message = "这是一段很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长很长的消息";
        let title = derive_thread_title(&thread(vec![message(1, "user", long_message)]));
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 41);
    }

    #[test]
    fn legacy_thread_migrates_to_unbound_conversation_schema() {
        let legacy = LegacyClassifiedAiThread {
            version: 1,
            thread_id: "019f0871-0000-7000-8000-000000000003".into(),
            document_path: ".classified/secret.md".into(),
            title: Some("旧会话".into()),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            messages: vec![
                message(1, "user", "请分析"),
                message(2, "assistant", "分析结果"),
            ],
            evidence_packets: vec![serde_json::json!({ "label": "[1]" })],
            token_usage: None,
        };

        let migrated = migrate_legacy_thread(legacy).expect("legacy thread migrates");
        let serialized = serde_json::to_value(&migrated).unwrap();

        assert_eq!(migrated.version, CLASSIFIED_SESSION_SCHEMA_VERSION);
        assert!(serialized.get("document_path").is_none());
        assert_eq!(migrated.turns.len(), 1);
        assert_eq!(migrated.runs.len(), 1);
        assert_eq!(migrated.events.len(), 1);
        assert_eq!(migrated.runs[0].status, "completed");
        assert_eq!(migrated.events[0].event_type, "completed");
        assert_eq!(migrated.runs[0].final_message_seq, Some(2));
        assert_eq!(migrated.evidence.len(), 1);
        assert_eq!(migrated.messages[0].turn_id, migrated.messages[1].turn_id);
    }

    #[test]
    fn classified_run_acceptance_persists_only_in_cef_conversation() {
        let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        {
            let mut key = VAULT_KEY
                .get()
                .expect("vault key initialized")
                .write()
                .expect("vault key write lock");
            key.set_test_key(test_key());
        }
        let vault = std::env::temp_dir().join(format!("iris-classified-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();

        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-client-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000010".into(),
                turn_id: "019f0871-0000-7000-8000-000000000011".into(),
                message: "涉密问题".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: Some(serde_json::json!({
                    "effect": "answer",
                    "selectionSnapshot": {
                        "referenceId": "explicit-selection",
                        "contentHash": "classified-content-hash",
                        "utf8Range": { "start": 0, "end": 4 },
                        "text": "仅本次涉密 Run 的选区"
                    }
                })),
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .expect("accept classified run");

        assert_eq!(accepted.session.domain, SecurityDomain::Classified);
        let stored = classified_ai_thread_load(&vault, accepted.session.session_key.clone())
            .expect("load classified conversation");
        assert_eq!(stored.messages.len(), 1);
        assert_eq!(
            stored.messages[0].turn_id.as_deref(),
            Some(accepted.turn_id.as_str())
        );
        assert_eq!(stored.runs.len(), 1);
        assert_eq!(
            stored.runs[0].client_request_id,
            "classified-client-request"
        );
        assert_eq!(
            stored.runs[0]
                .explicit_action
                .as_ref()
                .and_then(|action| action["selectionSnapshot"]["text"].as_str()),
            Some("仅本次涉密 Run 的选区")
        );
        assert_eq!(stored.events[0].event_type, "accepted");

        let duplicate = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-client-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000012".into(),
                turn_id: "019f0871-0000-7000-8000-000000000013".into(),
                message: "不应创建第二个涉密 Run".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .expect("idempotent acceptance");
        assert_eq!(duplicate.run_id, accepted.run_id);
        assert_eq!(duplicate.session, accepted.session);
        fs::remove_dir_all(vault).unwrap();
    }
    #[test]
    fn classified_run_get_and_cancel_replay_only_cef_events() {
        let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        {
            let mut key = VAULT_KEY
                .get()
                .expect("vault key initialized")
                .write()
                .expect("vault key write lock");
            key.set_test_key(test_key());
        }
        let vault = std::env::temp_dir().join(format!("iris-classified-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-replay-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000020".into(),
                turn_id: "019f0871-0000-7000-8000-000000000021".into(),
                message: "涉密问题".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .unwrap();

        let replay = classified_run_get(&vault, &accepted.session, &accepted.run_id)
            .unwrap()
            .expect("classified run");
        assert_eq!(replay.run.state, RunState::Accepted);
        assert_eq!(replay.events.len(), 1);

        let event = classified_run_cancel(
            &vault,
            &accepted.session,
            &accepted.run_id,
            replay.run.state_version,
        )
        .unwrap()
        .expect("cancel event");
        assert_eq!(serde_json::to_value(event).unwrap()["type"], "cancelled");

        let cancelled = classified_run_get(&vault, &accepted.session, &accepted.run_id)
            .unwrap()
            .expect("cancelled run");
        assert_eq!(cancelled.run.state, RunState::Cancelled);
        assert_eq!(cancelled.events.len(), 2);
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn unfinished_classified_run_is_terminalized_with_safe_events() {
        let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        {
            let mut key = VAULT_KEY
                .get()
                .expect("vault key initialized")
                .write()
                .expect("vault key write lock");
            key.set_test_key(test_key());
        }
        let vault = std::env::temp_dir().join(format!("iris-classified-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-unfinished-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000022".into(),
                turn_id: "019f0871-0000-7000-8000-000000000023".into(),
                message: "涉密问题".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .unwrap();

        let events = classified_run_fail_unfinished(
            &vault,
            &accepted.session,
            &accepted.run_id,
            crate::ai_runtime::run_contract::SafeRunErrorCode::PersistenceFailed,
        )
        .expect("terminalize unfinished run");

        assert_eq!(events.len(), 2);
        assert_eq!(
            serde_json::to_value(&events[0]).unwrap()["type"],
            "stage_changed"
        );
        assert_eq!(serde_json::to_value(&events[1]).unwrap()["type"], "failed");
        let replay = classified_run_get(&vault, &accepted.session, &accepted.run_id)
            .unwrap()
            .expect("terminalized run");
        assert_eq!(replay.run.state, RunState::Failed);
        assert_eq!(replay.run.state_version, 2);
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn classified_run_finalization_stays_in_cef_and_cannot_overwrite_a_cancellation() {
        let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        {
            let mut key = VAULT_KEY
                .get()
                .expect("vault key initialized")
                .write()
                .expect("vault key write lock");
            key.set_test_key(test_key());
        }
        let vault = std::env::temp_dir().join(format!("iris-classified-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-finalization-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000030".into(),
                turn_id: "019f0871-0000-7000-8000-000000000031".into(),
                message: "classified question".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .unwrap();

        let preparing = classified_run_mark_preparing(&vault, &accepted.session, &accepted.run_id)
            .expect("persist preparing");
        assert_eq!(
            serde_json::to_value(&preparing).unwrap()["type"],
            "stage_changed"
        );
        let running = classified_run_mark_running(&vault, &accepted.session, &accepted.run_id, 1)
            .expect("persist running");
        assert_eq!(serde_json::to_value(&running).unwrap()["stateVersion"], 2);

        let completed = classified_run_complete(
            &vault,
            &accepted.session,
            &accepted.run_id,
            2,
            "classified answer".into(),
        )
        .expect("persist classified completion");
        assert_eq!(
            serde_json::to_value(&completed).unwrap()["type"],
            "completed"
        );
        let replay = classified_run_get(&vault, &accepted.session, &accepted.run_id)
            .unwrap()
            .expect("completed run");
        assert_eq!(replay.run.state, RunState::Completed);
        assert!(replay.run.final_message_id.is_some());
        let stored =
            classified_ai_thread_load(&vault, accepted.session.session_key.clone()).unwrap();
        assert_eq!(stored.messages.len(), 2);
        assert_eq!(stored.messages[1].content, "classified answer");

        let second = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-cancel-before-complete".into(),
                session_key: Some(accepted.session.session_key.clone()),
                run_id: "019f0871-0000-7000-8000-000000000032".into(),
                turn_id: "019f0871-0000-7000-8000-000000000033".into(),
                message: "second classified question".into(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: serde_json::json!({ "securityDomain": "classified" }),
                effect: "answer".into(),
            },
        )
        .unwrap();
        let _ = classified_run_cancel(&vault, &second.session, &second.run_id, 0).unwrap();
        assert!(classified_run_complete(
            &vault,
            &second.session,
            &second.run_id,
            0,
            "must not be stored".into(),
        )
        .is_err());
        let stored = classified_ai_thread_load(&vault, second.session.session_key).unwrap();
        assert!(stored
            .messages
            .iter()
            .all(|message| message.content != "must not be stored"));
        fs::remove_dir_all(vault).unwrap();
    }
    #[test]
    fn retracting_a_classified_session_removes_the_turn_run_and_events() {
        let mut conversation = normalize_thread_for_write(thread(vec![
            message(1, "user", "first"),
            message(2, "assistant", "first answer"),
            message(3, "user", "second"),
            message(4, "assistant", "second answer"),
        ]))
        .unwrap();

        assert_eq!(retract_thread(&mut conversation, 3).unwrap(), 2);
        assert_eq!(conversation.messages.len(), 2);
        assert_eq!(conversation.turns.len(), 1);
        assert_eq!(conversation.runs.len(), 1);
        assert_eq!(conversation.events.len(), 1);
    }
    #[test]
    fn loading_a_v2_thread_persists_the_final_cef_schema_atomically() {
        let _test_lock = crate::crypto::vault_key::VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        {
            let mut key = VAULT_KEY
                .get()
                .expect("vault key initialized")
                .write()
                .expect("vault key write lock");
            key.set_test_key(test_key());
        }
        let vault = std::env::temp_dir().join(format!("iris-classified-v2-{}", Uuid::new_v4()));
        let key = test_key();
        fs::create_dir_all(sessions_dir(&vault)).unwrap();
        let mut legacy_v2 = thread(vec![message(1, "user", "历史问题")]);
        legacy_v2.version = 2;
        legacy_v2.turns.clear();
        legacy_v2.runs.clear();
        legacy_v2.events.clear();
        let path = thread_file_path(&vault, &legacy_v2.thread_id);
        fs::write(
            &path,
            classified_io::encrypt_cef(&serde_json::to_vec(&legacy_v2).unwrap(), &key).unwrap(),
        )
        .unwrap();

        let loaded = classified_ai_thread_load(&vault, legacy_v2.thread_id.clone()).unwrap();
        assert_eq!(loaded.version, CLASSIFIED_SESSION_SCHEMA_VERSION);
        assert_eq!(loaded.turns.len(), 1);
        assert_eq!(loaded.runs.len(), 1);
        assert_eq!(loaded.events.len(), 1);

        let persisted = classified_io::decrypt_cef(&fs::read(path).unwrap(), &key).unwrap();
        let persisted: ClassifiedAiThread = serde_json::from_slice(&persisted).unwrap();
        assert_eq!(persisted.version, CLASSIFIED_SESSION_SCHEMA_VERSION);
        assert_eq!(persisted.turns.len(), 1);
        fs::remove_dir_all(vault).unwrap();
    }
    #[test]
    fn atomic_write_validates_and_replaces_an_existing_cef() {
        let root = std::env::temp_dir().join(format!("iris-classified-session-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("thread.cef");
        let key = test_key();
        fs::write(&path, b"old-cef").unwrap();
        let migrated = migrate_legacy_thread(LegacyClassifiedAiThread {
            version: 1,
            thread_id: "019f0871-0000-7000-8000-000000000004".into(),
            document_path: ".classified/old.md".into(),
            title: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            messages: vec![message(1, "user", "旧内容")],
            evidence_packets: vec![],
            token_usage: None,
        })
        .unwrap();

        write_thread_atomically(&path, &migrated, &key).unwrap();
        let raw = fs::read(&path).unwrap();
        let plaintext = classified_io::decrypt_cef(&raw, &key).unwrap();
        let stored: ClassifiedAiThread = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(stored.version, CLASSIFIED_SESSION_SCHEMA_VERSION);
        assert!(serde_json::to_value(&stored)
            .unwrap()
            .get("document_path")
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn atomic_write_keeps_the_previous_cef_when_validation_fails() {
        let root = std::env::temp_dir().join(format!("iris-classified-session-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("thread.cef");
        let key = test_key();
        let previous = b"previous-cef";
        fs::write(&path, previous).unwrap();
        let mut invalid = thread(vec![]);
        invalid.thread_id = "not-a-uuid".into();

        assert!(write_thread_atomically(&path, &invalid, &key).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        fs::remove_dir_all(root).unwrap();
    }
}
