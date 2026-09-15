//! C26 安全边界事件：关联标识、出站结构见证与记录完整性。
//!
//! 本模块只追加诊断事件，不保存第二套 Run 生命周期状态。结构见证只证明
//! 必要信息的传递与关联，不证明语义理解。不记录密钥、笔记正文、完整模型
//! 请求或私有推理。

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ai_types::{LlmMessage, MessageRole};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

static PERSIST_FAILURES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn persist_failures() -> &'static Mutex<HashSet<String>> {
    PERSIST_FAILURES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn mark_persist_failed(run_id: &str) {
    if let Ok(mut failed) = persist_failures().lock() {
        failed.insert(run_id.to_string());
    }
}

/// Whether C26 persistence failed for this Run in the current process.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "C27 diagnostic health surface; exercised by C26 tests"
    )
)]
pub fn persist_failed(run_id: &str) -> bool {
    persist_failures()
        .lock()
        .map(|failed| failed.contains(run_id))
        .unwrap_or(true)
}

/// Closed correlation identity required on every C26 record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryCorrelation {
    pub run_id: String,
    pub input_revision: String,
    pub parent_run_id: Option<String>,
    pub child_run_id: Option<String>,
    pub model_turn: u32,
    pub call_id: String,
    pub attempt_id: String,
    pub tool_surface_version: String,
    pub protocol_adapter: String,
}

impl BoundaryCorrelation {
    fn validate(&self) -> AppResult<()> {
        if self.run_id.trim().is_empty()
            || self.input_revision.trim().is_empty()
            || self.call_id.trim().is_empty()
            || self.attempt_id.trim().is_empty()
            || self.tool_surface_version.trim().is_empty()
            || self.protocol_adapter.trim().is_empty()
        {
            return Err(AppError::msg("broken_correlation"));
        }
        Ok(())
    }
}

/// Protocol family of one outbound model request. Names the adapter, not a vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolFamily {
    OpenAiChatCompletions,
    AnthropicMessages,
    OpenAiResponses,
}

impl ProtocolFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::AnthropicMessages => "anthropic_messages",
            Self::OpenAiResponses => "openai_responses",
        }
    }
}

/// Which of the four K17 transmission layers a record describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryLayer {
    Generated,
    Serialized,
    RequestSent,
    ProviderReturned,
}

impl BoundaryLayer {
    fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Serialized => "serialized",
            Self::RequestSent => "request_sent",
            Self::ProviderReturned => "provider_returned",
        }
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "used by query_by_run, C27 read surface")
    )]
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "generated" => Ok(Self::Generated),
            "serialized" => Ok(Self::Serialized),
            "request_sent" => Ok(Self::RequestSent),
            "provider_returned" => Ok(Self::ProviderReturned),
            _ => Err(AppError::msg("invalid_boundary_layer")),
        }
    }
}

/// Event kinds owned by C26. Attribution stays in C27.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryEventKind {
    HandshakeStart,
    HandshakeEnd,
    OutboundWitness,
    PersistFailed,
    MissingEnd,
    LoopEvent,
}

impl BoundaryEventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::HandshakeStart => "handshake_start",
            Self::HandshakeEnd => "handshake_end",
            Self::OutboundWitness => "outbound_witness",
            Self::PersistFailed => "persist_failed",
            Self::MissingEnd => "missing_end",
            Self::LoopEvent => "loop_event",
        }
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "used by query_by_run, C27 read surface")
    )]
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "handshake_start" => Ok(Self::HandshakeStart),
            "handshake_end" => Ok(Self::HandshakeEnd),
            "outbound_witness" => Ok(Self::OutboundWitness),
            "persist_failed" => Ok(Self::PersistFailed),
            "missing_end" => Ok(Self::MissingEnd),
            "loop_event" => Ok(Self::LoopEvent),
            _ => Err(AppError::msg("invalid_boundary_event_kind")),
        }
    }
}

/// Record completeness discrete values from K17.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordCompleteness {
    Complete,
    MissingEvents,
    BrokenCorrelation,
    PersistFailed,
    OutcomeUnknown,
}

impl RecordCompleteness {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::MissingEvents => "missing-events",
            Self::BrokenCorrelation => "broken-correlation",
            Self::PersistFailed => "persist-failed",
            Self::OutcomeUnknown => "outcome-unknown",
        }
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "used by query_by_run, C27 read surface")
    )]
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "complete" => Ok(Self::Complete),
            "missing-events" => Ok(Self::MissingEvents),
            "broken-correlation" => Ok(Self::BrokenCorrelation),
            "persist-failed" => Ok(Self::PersistFailed),
            "outcome-unknown" => Ok(Self::OutcomeUnknown),
            _ => Err(AppError::msg("invalid_record_completeness")),
        }
    }
}

/// Discovery location. This is not a root-cause field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryLocation {
    pub module: &'static str,
    pub component: &'static str,
    pub tool_instance: Option<String>,
}

/// Role counts only. Message bodies never enter this struct.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageRoleCounts {
    pub system: u32,
    pub user: u32,
    pub assistant: u32,
    pub tool: u32,
}

/// Closed presence flags for required outbound fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequiredFieldsPresent {
    pub model: bool,
    pub messages: bool,
    pub tools: bool,
}

/// Safe outbound structure witness. Proves transmission, not understanding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutboundStructureWitness {
    pub schema_version: u8,
    pub protocol_family: ProtocolFamily,
    pub message_role_counts: MessageRoleCounts,
    pub required_fields_present: RequiredFieldsPresent,
    pub tool_call_pairing_valid: bool,
    pub repair_included: bool,
    pub tool_count: u32,
}

/// Provider-return structure without bodies or error strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderReturnStructure {
    pub http_status_class: Option<&'static str>,
    pub has_content: bool,
    pub tool_call_count: u32,
    pub finish_reason_class: Option<&'static str>,
}

/// One persisted C26 record. Payload is closed JSON, never a model request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "C27 diagnostic query surface; exercised by C26 tests"
    )
)]
pub struct BoundaryEventRecord {
    pub run_id: String,
    pub input_revision: String,
    pub parent_run_id: Option<String>,
    pub child_run_id: Option<String>,
    pub model_turn: u32,
    pub call_id: String,
    pub attempt_id: String,
    pub tool_surface_version: String,
    pub protocol_adapter: String,
    pub layer: BoundaryLayer,
    pub event_kind: BoundaryEventKind,
    pub record_completeness: RecordCompleteness,
    pub module_id: String,
    pub component_id: String,
    pub payload: serde_json::Value,
}

#[derive(Default)]
struct SlotTrace {
    generated: Option<OutboundStructureWitness>,
    serialized: Option<OutboundStructureWitness>,
    request_sent: bool,
    provider_returned: Option<ProviderReturnStructure>,
    handshake_end: Option<RecordCompleteness>,
    recorded_layers: HashSet<BoundaryLayer>,
}

/// In-memory handshake slot shared between the gateway and C26 persistence.
#[derive(Clone)]
pub struct BoundaryAuditSlot {
    correlation: BoundaryCorrelation,
    inner: Arc<Mutex<SlotTrace>>,
}

impl std::fmt::Debug for BoundaryAuditSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundaryAuditSlot")
            .field("correlation", &self.correlation)
            .finish_non_exhaustive()
    }
}

impl BoundaryAuditSlot {
    /// Create a slot for one model-call handshake.
    pub fn new(correlation: BoundaryCorrelation) -> Self {
        Self {
            correlation,
            inner: Arc::new(Mutex::new(SlotTrace::default())),
        }
    }

    /// Correlation identity attached to this handshake.
    pub fn correlation(&self) -> &BoundaryCorrelation {
        &self.correlation
    }

    /// Record the business-layer transcript shape (no bodies).
    pub fn note_generated(&self, messages: &[LlmMessage], tool_count: u32, model_present: bool) {
        if let Ok(mut trace) = self.inner.lock() {
            trace.generated = Some(outbound_witness_from_messages(
                protocol_family_from_adapter(&self.correlation.protocol_adapter),
                messages,
                tool_count,
                model_present,
            ));
        }
    }

    /// Record the serialized request shape from the actual JSON body.
    pub fn note_serialized(
        &self,
        family: ProtocolFamily,
        body: &serde_json::Value,
        generated_messages: &[LlmMessage],
    ) {
        if let Ok(mut trace) = self.inner.lock() {
            trace.serialized = Some(outbound_witness_from_serialized_body(
                family,
                body,
                generated_messages,
            ));
        }
    }

    /// Record that the HTTP request left the process.
    pub fn note_request_sent(&self) {
        if let Ok(mut trace) = self.inner.lock() {
            trace.request_sent = true;
        }
    }

    /// Record provider-return structure without bodies.
    pub fn note_provider_returned(&self, returned: ProviderReturnStructure) {
        if let Ok(mut trace) = self.inner.lock() {
            trace.provider_returned = Some(returned);
        }
    }

    /// Record a handshake termination. Completeness refers to the record, not task success.
    pub fn note_handshake_end(&self, completeness: RecordCompleteness) {
        if let Ok(mut trace) = self.inner.lock() {
            if trace.handshake_end.is_none() {
                trace.handshake_end = Some(completeness);
            }
        }
    }

    /// Record a known unsuccessful terminal (cancel, timeout, transport) without
    /// inferring task success. No-ops if a more specific end was already noted.
    pub fn note_known_failure(&self, finish_reason_class: &'static str) {
        if self.handshake_closed() {
            return;
        }
        self.note_provider_returned(ProviderReturnStructure {
            http_status_class: None,
            has_content: false,
            tool_call_count: 0,
            finish_reason_class: Some(finish_reason_class),
        });
        self.note_handshake_end(RecordCompleteness::Complete);
    }

    fn handshake_closed(&self) -> bool {
        self.inner
            .lock()
            .ok()
            .is_some_and(|trace| trace.handshake_end.is_some())
    }
}

/// Guard that persists remaining handshake layers when a model call returns or unwinds.
pub struct BoundaryPersistGuard<'a> {
    db: &'a Database,
    slot: BoundaryAuditSlot,
}

impl<'a> BoundaryPersistGuard<'a> {
    /// Persist remaining layers from `slot` when dropped.
    pub fn new(db: &'a Database, slot: BoundaryAuditSlot) -> Self {
        Self { db, slot }
    }
}

impl Drop for BoundaryPersistGuard<'_> {
    fn drop(&mut self) {
        if persist_slot_updates(self.db, &self.slot).is_err() {
            mark_persist_failed(&self.slot.correlation.run_id);
        }
    }
}

fn protocol_family_from_adapter(adapter: &str) -> ProtocolFamily {
    match adapter {
        "anthropic_messages" => ProtocolFamily::AnthropicMessages,
        "openai_responses" => ProtocolFamily::OpenAiResponses,
        _ => ProtocolFamily::OpenAiChatCompletions,
    }
}

/// Build a content-free witness from the business-layer message list.
pub fn outbound_witness_from_messages(
    protocol_family: ProtocolFamily,
    messages: &[LlmMessage],
    tool_count: u32,
    model_present: bool,
) -> OutboundStructureWitness {
    let message_role_counts = count_message_roles(messages);
    let tool_call_pairing_valid = tool_pairing_valid(messages);
    let repair_included = message_role_counts.tool > 0 && tool_call_pairing_valid;
    OutboundStructureWitness {
        schema_version: 1,
        protocol_family,
        message_role_counts,
        required_fields_present: RequiredFieldsPresent {
            model: model_present,
            messages: !messages.is_empty(),
            tools: tool_count > 0,
        },
        tool_call_pairing_valid,
        repair_included,
        tool_count,
    }
}

/// Build a content-free witness from the serialized provider body.
pub fn outbound_witness_from_serialized_body(
    protocol_family: ProtocolFamily,
    body: &serde_json::Value,
    generated_messages: &[LlmMessage],
) -> OutboundStructureWitness {
    let message_role_counts = count_serialized_roles(body);
    let generated_had_tool_feedback = generated_messages
        .iter()
        .any(|message| matches!(message.role, MessageRole::Tool));
    let serialized_has_tool = message_role_counts.tool > 0 || serialized_has_tool_result(body);
    let tool_count = serialized_tool_count(body);
    let model_present = json_nonempty_str(body.get("model"));
    let messages_present = body
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .is_some()
        || body
            .get("input")
            .and_then(serde_json::Value::as_array)
            .is_some();
    OutboundStructureWitness {
        schema_version: 1,
        protocol_family,
        message_role_counts,
        required_fields_present: RequiredFieldsPresent {
            model: model_present,
            messages: messages_present,
            tools: tool_count > 0,
        },
        tool_call_pairing_valid: tool_pairing_valid(generated_messages),
        repair_included: generated_had_tool_feedback && serialized_has_tool,
        tool_count,
    }
}

fn count_message_roles(messages: &[LlmMessage]) -> MessageRoleCounts {
    let mut counts = MessageRoleCounts::default();
    for message in messages {
        match message.role {
            MessageRole::System => counts.system = counts.system.saturating_add(1),
            MessageRole::User => counts.user = counts.user.saturating_add(1),
            MessageRole::Assistant => counts.assistant = counts.assistant.saturating_add(1),
            MessageRole::Tool => counts.tool = counts.tool.saturating_add(1),
        }
    }
    counts
}

fn tool_pairing_valid(messages: &[LlmMessage]) -> bool {
    messages.iter().enumerate().all(|(index, message)| {
        if !matches!(message.role, MessageRole::Tool) {
            return true;
        }
        let Some(tool_id) = message.tool_call_id.as_deref().filter(|id| !id.is_empty()) else {
            return false;
        };
        messages[..index].iter().rev().any(|parent| {
            matches!(parent.role, MessageRole::Assistant)
                && parent
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| calls.iter().any(|call| call.id == tool_id))
        })
    })
}

fn count_serialized_roles(body: &serde_json::Value) -> MessageRoleCounts {
    let mut counts = MessageRoleCounts::default();
    if json_nonempty_str(body.get("system")) || body.get("instructions").is_some() {
        counts.system = counts.system.saturating_add(1);
    }
    let rows = body
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .or_else(|| body.get("input").and_then(serde_json::Value::as_array));
    let Some(rows) = rows else {
        return counts;
    };
    for row in rows {
        match row.get("role").and_then(serde_json::Value::as_str) {
            Some("system") => counts.system = counts.system.saturating_add(1),
            Some("user") => counts.user = counts.user.saturating_add(1),
            Some("assistant") => counts.assistant = counts.assistant.saturating_add(1),
            Some("tool") => counts.tool = counts.tool.saturating_add(1),
            _ => {
                if row.get("type").and_then(serde_json::Value::as_str)
                    == Some("function_call_output")
                    || row.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                {
                    counts.tool = counts.tool.saturating_add(1);
                }
            }
        }
    }
    counts
}

fn serialized_has_tool_result(body: &serde_json::Value) -> bool {
    count_serialized_roles(body).tool > 0
}

fn serialized_tool_count(body: &serde_json::Value) -> u32 {
    body.get("tools")
        .and_then(serde_json::Value::as_array)
        .map(|tools| u32::try_from(tools.len()).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

fn json_nonempty_str(value: Option<&serde_json::Value>) -> bool {
    value
        .and_then(serde_json::Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

/// Hash frozen tool names into a short surface version token.
pub fn tool_surface_version(tool_names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    use sha2::{Digest, Sha256};
    let mut names: Vec<String> = tool_names
        .into_iter()
        .map(|name| name.as_ref().to_string())
        .collect();
    names.sort();
    names.dedup();
    let digest = Sha256::digest(names.join("\n").as_bytes());
    format!("sha256:{}", &hex::encode(digest)[..16])
}

/// Append one C26 event. Incomplete correlation is rejected, not stored as success.
pub fn record_event(
    db: &Database,
    correlation: &BoundaryCorrelation,
    layer: BoundaryLayer,
    event_kind: BoundaryEventKind,
    record_completeness: RecordCompleteness,
    discovery: DiscoveryLocation,
    payload: serde_json::Value,
) -> AppResult<()> {
    correlation.validate()?;
    let payload = sanitize_payload(payload);
    let result = db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO audit_boundary_events (
                run_id, input_revision, parent_run_id, child_run_id, model_turn,
                call_id, attempt_id, tool_surface_version, protocol_adapter,
                layer, event_kind, record_completeness, module_id, component_id,
                tool_instance, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            rusqlite::params![
                correlation.run_id,
                correlation.input_revision,
                correlation.parent_run_id,
                correlation.child_run_id,
                correlation.model_turn as i64,
                correlation.call_id,
                correlation.attempt_id,
                correlation.tool_surface_version,
                correlation.protocol_adapter,
                layer.as_str(),
                event_kind.as_str(),
                record_completeness.as_str(),
                discovery.module,
                discovery.component,
                discovery.tool_instance,
                serde_json::to_string(&payload)?,
            ],
        )?;
        Ok(())
    });
    if result.is_err() {
        mark_persist_failed(&correlation.run_id);
    }
    result
}

/// Persist one in-memory slot layer if it has not already been written.
pub fn record_slot_layer(
    db: &Database,
    slot: &BoundaryAuditSlot,
    layer: BoundaryLayer,
) -> AppResult<()> {
    let trace = slot
        .inner
        .lock()
        .map_err(|_| AppError::msg("boundary_slot_lock_failed"))?;
    if trace.recorded_layers.contains(&layer) {
        return Ok(());
    }
    let (kind, completeness, payload) = match layer {
        BoundaryLayer::Generated => {
            let Some(witness) = trace.generated.clone() else {
                return Ok(());
            };
            (
                BoundaryEventKind::OutboundWitness,
                RecordCompleteness::Complete,
                serde_json::to_value(&witness)?,
            )
        }
        BoundaryLayer::Serialized => {
            let Some(witness) = trace.serialized.clone() else {
                return Ok(());
            };
            (
                BoundaryEventKind::OutboundWitness,
                RecordCompleteness::Complete,
                serde_json::to_value(&witness)?,
            )
        }
        BoundaryLayer::RequestSent => {
            if !trace.request_sent {
                return Ok(());
            }
            (
                BoundaryEventKind::OutboundWitness,
                RecordCompleteness::Complete,
                serde_json::json!({"schemaVersion": 1, "requestSent": true}),
            )
        }
        BoundaryLayer::ProviderReturned => {
            let Some(returned) = trace.provider_returned else {
                return Ok(());
            };
            (
                BoundaryEventKind::OutboundWitness,
                RecordCompleteness::Complete,
                serde_json::json!({
                    "schemaVersion": 1,
                    "httpStatusClass": returned.http_status_class,
                    "hasContent": returned.has_content,
                    "toolCallCount": returned.tool_call_count,
                    "finishReasonClass": returned.finish_reason_class,
                }),
            )
        }
    };
    drop(trace);
    let result = record_event(
        db,
        &slot.correlation,
        layer,
        kind,
        completeness,
        DiscoveryLocation {
            module: "M04",
            component: "C11",
            tool_instance: None,
        },
        payload,
    );
    if result.is_ok() {
        if let Ok(mut trace) = slot.inner.lock() {
            trace.recorded_layers.insert(layer);
        }
    }
    result
}

/// Persist handshake termination for a slot.
pub fn record_handshake_end(db: &Database, slot: &BoundaryAuditSlot) -> AppResult<()> {
    let completeness = slot
        .inner
        .lock()
        .ok()
        .and_then(|trace| trace.handshake_end)
        .unwrap_or(RecordCompleteness::OutcomeUnknown);
    record_event(
        db,
        &slot.correlation,
        BoundaryLayer::ProviderReturned,
        BoundaryEventKind::HandshakeEnd,
        completeness,
        DiscoveryLocation {
            module: "M04",
            component: "C11",
            tool_instance: None,
        },
        serde_json::json!({"schemaVersion": 1}),
    )
}

fn persist_slot_updates(db: &Database, slot: &BoundaryAuditSlot) -> AppResult<()> {
    record_slot_layer(db, slot, BoundaryLayer::Generated)?;
    record_slot_layer(db, slot, BoundaryLayer::Serialized)?;
    record_slot_layer(db, slot, BoundaryLayer::RequestSent)?;
    record_slot_layer(db, slot, BoundaryLayer::ProviderReturned)?;
    let has_end = slot
        .inner
        .lock()
        .ok()
        .is_some_and(|trace| trace.handshake_end.is_some());
    if has_end {
        record_handshake_end(db, slot)?;
    } else {
        record_event(
            db,
            &slot.correlation,
            BoundaryLayer::ProviderReturned,
            BoundaryEventKind::MissingEnd,
            RecordCompleteness::OutcomeUnknown,
            DiscoveryLocation {
                module: "M09",
                component: "C26",
                tool_instance: None,
            },
            serde_json::json!({"schemaVersion": 1}),
        )?;
        if let Ok(mut trace) = slot.inner.lock() {
            trace.handshake_end = Some(RecordCompleteness::OutcomeUnknown);
        }
    }
    Ok(())
}

/// Persist a tool-loop diagnostic event without dropping earlier recovery rows.
pub fn record_loop_event(
    db: &Database,
    correlation: &BoundaryCorrelation,
    event: &serde_json::Value,
) -> AppResult<()> {
    let mut correlation = correlation.clone();
    let existing = count_loop_events(db, &correlation.run_id).unwrap_or(0);
    correlation.attempt_id = format!("loop-{}", existing.saturating_add(1));
    if correlation.call_id.trim().is_empty() {
        correlation.call_id = event
            .get("tool")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("loop")
            .to_string();
    }
    if let Some(turns) = event.get("modelTurns").and_then(serde_json::Value::as_u64) {
        correlation.model_turn = u32::try_from(turns).unwrap_or(u32::MAX);
    }
    record_event(
        db,
        &correlation,
        BoundaryLayer::Generated,
        BoundaryEventKind::LoopEvent,
        RecordCompleteness::Complete,
        DiscoveryLocation {
            module: "M05",
            component: "C14",
            tool_instance: event
                .get("tool")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        },
        sanitize_loop_event(event),
    )
}

fn count_loop_events(db: &Database, run_id: &str) -> AppResult<u32> {
    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM audit_boundary_events
             WHERE run_id = ?1 AND event_kind = 'loop_event'",
            [run_id],
            |row| row.get(0),
        )?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    })
}

fn sanitize_loop_event(event: &serde_json::Value) -> serde_json::Value {
    const ALLOWED: &[&str] = &[
        "event",
        "round",
        "tool",
        "reason",
        "modelTurns",
        "toolCalls",
        "success",
        "noProgressRounds",
        "failedServiceRounds",
        "catalogKnown",
        "observationPerformed",
        "capabilityBlocked",
        "providerAttempts",
        "rejectedProposals",
        "repairRounds",
    ];
    let Some(object) = event.as_object() else {
        return serde_json::json!({"event": "loop"});
    };
    let mut safe = serde_json::Map::new();
    for key in ALLOWED {
        if let Some(value) = object.get(*key) {
            if value.is_boolean() || value.is_number() || value.is_null() {
                safe.insert((*key).to_string(), value.clone());
            } else if let Some(text) = value.as_str() {
                if text.len() <= 64 && !looks_sensitive(text) {
                    safe.insert(
                        (*key).to_string(),
                        serde_json::Value::String(text.to_string()),
                    );
                }
            }
        }
    }
    if safe.is_empty() {
        safe.insert("event".into(), serde_json::json!("loop"));
    }
    serde_json::Value::Object(safe)
}

fn looks_sensitive(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.contains("http://")
        || lower.contains("https://")
        || text.contains('\n')
}

fn sanitize_payload(payload: serde_json::Value) -> serde_json::Value {
    fn walk(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                if looks_sensitive(text) {
                    *text = "redacted".into();
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(walk),
            serde_json::Value::Object(object) => {
                object.values_mut().for_each(walk);
            }
            _ => {}
        }
    }
    let mut payload = payload;
    walk(&mut payload);
    payload
}

/// Mark open handshakes as outcome-unknown instead of inferring success.
pub fn close_open_handshakes(db: &Database, run_id: &str) -> AppResult<()> {
    let open: Vec<BoundaryCorrelation> = db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT input_revision, parent_run_id, child_run_id, model_turn, call_id,
                    attempt_id, tool_surface_version, protocol_adapter
             FROM audit_boundary_events
             WHERE run_id = ?1 AND event_kind = 'handshake_start'
               AND attempt_id NOT IN (
                    SELECT attempt_id FROM audit_boundary_events
                    WHERE run_id = ?1
                      AND event_kind IN ('handshake_end', 'missing_end')
               )",
        )?;
        let rows = statement.query_map([run_id], |row| {
            Ok(BoundaryCorrelation {
                run_id: run_id.to_string(),
                input_revision: row.get(0)?,
                parent_run_id: row.get(1)?,
                child_run_id: row.get(2)?,
                model_turn: u32::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                call_id: row.get(4)?,
                attempt_id: row.get(5)?,
                tool_surface_version: row.get(6)?,
                protocol_adapter: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })?;
    for correlation in open {
        record_event(
            db,
            &correlation,
            BoundaryLayer::ProviderReturned,
            BoundaryEventKind::MissingEnd,
            RecordCompleteness::OutcomeUnknown,
            DiscoveryLocation {
                module: "M09",
                component: "C26",
                tool_instance: None,
            },
            serde_json::json!({"schemaVersion": 1}),
        )?;
    }
    Ok(())
}

/// Compute the strongest completeness gap for one Run.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "C27 diagnostic query surface; exercised by C26 tests"
    )
)]
pub fn assess_completeness(db: &Database, run_id: &str) -> AppResult<RecordCompleteness> {
    if persist_failed(run_id) {
        return Ok(RecordCompleteness::PersistFailed);
    }
    let events = query_by_run(db, run_id)?;
    if events.is_empty() {
        return Ok(RecordCompleteness::MissingEvents);
    }
    if events
        .iter()
        .any(|event| event.record_completeness == RecordCompleteness::PersistFailed)
    {
        return Ok(RecordCompleteness::PersistFailed);
    }
    if events
        .iter()
        .any(|event| event.record_completeness == RecordCompleteness::BrokenCorrelation)
    {
        return Ok(RecordCompleteness::BrokenCorrelation);
    }
    if events.iter().any(|event| {
        matches!(
            event.event_kind,
            BoundaryEventKind::MissingEnd | BoundaryEventKind::PersistFailed
        ) || event.record_completeness == RecordCompleteness::OutcomeUnknown
    }) {
        return Ok(RecordCompleteness::OutcomeUnknown);
    }
    let starts = events
        .iter()
        .filter(|event| event.event_kind == BoundaryEventKind::HandshakeStart)
        .count();
    let ends = events
        .iter()
        .filter(|event| {
            matches!(
                event.event_kind,
                BoundaryEventKind::HandshakeEnd | BoundaryEventKind::MissingEnd
            )
        })
        .count();
    if starts > ends {
        return Ok(RecordCompleteness::OutcomeUnknown);
    }
    Ok(RecordCompleteness::Complete)
}

/// Read C26 events for one Run. Query failure is an error, never an empty success.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "C27 diagnostic query surface; exercised by C26 tests"
    )
)]
pub fn query_by_run(db: &Database, run_id: &str) -> AppResult<Vec<BoundaryEventRecord>> {
    db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT run_id, input_revision, parent_run_id, child_run_id, model_turn,
                    call_id, attempt_id, tool_surface_version, protocol_adapter,
                    layer, event_kind, record_completeness, module_id, component_id,
                    payload_json
             FROM audit_boundary_events
             WHERE run_id = ?1
             ORDER BY id",
        )?;
        let rows = statement.query_map([run_id], |row| {
            let payload_json: String = row.get(14)?;
            Ok((
                BoundaryEventRecord {
                    run_id: row.get(0)?,
                    input_revision: row.get(1)?,
                    parent_run_id: row.get(2)?,
                    child_run_id: row.get(3)?,
                    model_turn: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                    call_id: row.get(5)?,
                    attempt_id: row.get(6)?,
                    tool_surface_version: row.get(7)?,
                    protocol_adapter: row.get(8)?,
                    layer: BoundaryLayer::Generated,
                    event_kind: BoundaryEventKind::LoopEvent,
                    record_completeness: RecordCompleteness::Complete,
                    module_id: row.get(12)?,
                    component_id: row.get(13)?,
                    payload: serde_json::Value::Null,
                },
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                payload_json,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (mut event, layer, kind, completeness, payload_json) = row?;
            event.layer = BoundaryLayer::parse(&layer).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(error.to_string())),
                )
            })?;
            event.event_kind = BoundaryEventKind::parse(&kind).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::other(error.to_string())),
                )
            })?;
            event.record_completeness =
                RecordCompleteness::parse(&completeness).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        11,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::other(error.to_string())),
                    )
                })?;
            event.payload = serde_json::from_str(&payload_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    14,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            events.push(event);
        }
        Ok(events)
    })
}

/// Best-effort handshake start used by the production model path.
pub fn record_handshake_start(db: &Database, slot: &BoundaryAuditSlot) -> AppResult<()> {
    record_event(
        db,
        &slot.correlation,
        BoundaryLayer::Generated,
        BoundaryEventKind::HandshakeStart,
        RecordCompleteness::Complete,
        DiscoveryLocation {
            module: "M04",
            component: "C11",
            tool_instance: None,
        },
        serde_json::json!({"schemaVersion": 1}),
    )?;
    record_slot_layer(db, slot, BoundaryLayer::Generated)
}

/// Protocol adapter token stored on correlation rows.
pub fn protocol_adapter_name(family: ProtocolFamily) -> &'static str {
    family.as_str()
}

/// Next model-turn index for handshake correlation (1-based).
pub fn next_handshake_turn(db: &Database, run_id: &str) -> u32 {
    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM audit_boundary_events
             WHERE run_id = ?1 AND event_kind = 'handshake_start'",
            [run_id],
            |row| row.get(0),
        )?;
        Ok(u32::try_from(count).unwrap_or(0).saturating_add(1))
    })
    .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
    use crate::ai_runtime::model_gateway::{GatewayRequest, LlmFunctionDef, LlmToolDef};
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::run_contract::{
        ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality,
        RiskClass, SecurityDomain, WebDecisionReason,
    };
    use crate::ai_types::{
        EndpointFamily, FunctionCall, LlmMessage, MessageRole, ProviderConfig, ToolCall,
    };
    use crate::storage::db::Database;

    const SECRET_NOTE: &str = "UNIQUE_NOTE_BODY_SHOULD_NEVER_ENTER_C26_ab3f91";
    const SECRET_KEY: &str = "sk-live-should-never-be-logged-c26";

    fn sample_provider() -> ProviderConfig {
        ProviderConfig {
            name: "contract".into(),
            base_url: "https://api.example.test/v1".into(),
            api_key: Some(zeroize::Zeroizing::new(SECRET_KEY.to_string())),
            model: "contract-model".into(),
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        }
    }

    fn messages_with_secret_note() -> Vec<LlmMessage> {
        vec![
            LlmMessage {
                role: MessageRole::System,
                content: "system boundary".into(),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::User,
                content: SECRET_NOTE.into(),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Assistant,
                content: String::new().into(),
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".into(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "read_note".into(),
                        arguments: r#"{"path":"notes/secret.md"}"#.into(),
                    },
                }]),
                reasoning_content: Some("private-reasoning-must-not-persist".into()),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: SECRET_NOTE.into(),
                tool_call_id: Some("call_1".into()),
                ..Default::default()
            },
        ]
    }

    fn closed_correlation(run_id: &str) -> BoundaryCorrelation {
        BoundaryCorrelation {
            run_id: run_id.to_string(),
            input_revision: "turn-1".into(),
            parent_run_id: None,
            child_run_id: None,
            model_turn: 1,
            call_id: "call_1".into(),
            attempt_id: "attempt_1".into(),
            tool_surface_version: "surface-v1".into(),
            protocol_adapter: "openai_chat_completions".into(),
        }
    }

    fn accept_run(db: &Database, run_id: &str) -> String {
        let session = NormalSessionRepository::create(db).expect("session");
        AgentRunRepository::accept(
            db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key,
                client_request_id: format!("{run_id}-client"),
                run_id: run_id.to_string(),
                turn_id: format!("{run_id}-turn"),
                message: "diagnose this run".into(),
                content_parts: None,
                explicit_references: vec![],
                context_scope: Default::default(),
                display_mentions: vec![],
                explicit_action: None,
                envelope: ExecutionEnvelope {
                    effect: Effect::Answer,
                    context: ContextMode::ExplicitReferences,
                    freshness: Freshness::Offline,
                    web_reason: WebDecisionReason::LegacyUnknown,
                    verification_requirement:
                        crate::ai_runtime::run_contract::VerificationRequirement::None,
                    effort: Effort::Direct,
                    security_domain: SecurityDomain::Normal,
                    risk: RiskClass::ReadOnly,
                    modalities: vec![Modality::Text],
                    material_needs: vec![MaterialNeed::Reference],
                    required_capabilities: vec![],
                    explicit_constraints: vec![],
                    fresh_fact: Default::default(),
                },
            },
        )
        .expect("accepted")
        .run_id
    }

    #[test]
    fn outbound_witness_excludes_note_bodies_secrets_and_private_reasoning() {
        let witness = outbound_witness_from_messages(
            ProtocolFamily::OpenAiChatCompletions,
            &messages_with_secret_note(),
            1,
            true,
        );
        let encoded = serde_json::to_string(&witness).expect("json");
        assert!(!encoded.contains(SECRET_NOTE), "{encoded}");
        assert!(!encoded.contains(SECRET_KEY), "{encoded}");
        assert!(
            !encoded.contains("private-reasoning-must-not-persist"),
            "{encoded}"
        );
        assert!(!encoded.contains("notes/secret.md"), "{encoded}");
        assert_eq!(witness.schema_version, 1);
        assert_eq!(witness.message_role_counts.system, 1);
        assert_eq!(witness.message_role_counts.user, 1);
        assert_eq!(witness.message_role_counts.assistant, 1);
        assert_eq!(witness.message_role_counts.tool, 1);
        assert!(witness.repair_included);
        assert!(witness.required_fields_present.messages);
        assert!(witness.required_fields_present.model);
    }

    #[test]
    fn serialized_witness_from_body_never_copies_message_text() {
        let request = GatewayRequest {
            provider: sample_provider(),
            messages: messages_with_secret_note(),
            tools: vec![LlmToolDef {
                tool_type: "function".into(),
                function: LlmFunctionDef {
                    name: "read_note".into(),
                    description: "read".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }],
            max_tokens: Some(32),
            input_token_budget: None,
            temperature: None,
            stream: false,
            thinking: false,
            reasoning: crate::ai_types::ResolvedReasoningRequest::disabled(),
            continuation: None,
            skip_stub_ids: vec![],
            boundary: None,
        };
        let body = crate::ai_runtime::model_gateway::build_chat_completions_body(&request);
        let witness = outbound_witness_from_serialized_body(
            ProtocolFamily::OpenAiChatCompletions,
            &body,
            &messages_with_secret_note(),
        );
        let encoded = serde_json::to_string(&witness).expect("json");
        assert!(!encoded.contains(SECRET_NOTE), "{encoded}");
        assert!(witness.required_fields_present.model);
        assert!(witness.required_fields_present.messages);
        assert!(witness.required_fields_present.tools);
        assert!(witness.repair_included);
    }

    #[test]
    fn repair_is_not_claimed_when_tool_feedback_is_absent() {
        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: "hello".into(),
            ..Default::default()
        }];
        let witness = outbound_witness_from_messages(
            ProtocolFamily::OpenAiChatCompletions,
            &messages,
            0,
            true,
        );
        assert!(!witness.repair_included);
        assert_eq!(witness.message_role_counts.tool, 0);
    }

    #[test]
    fn correlation_fields_are_required_before_persist() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-missing-correlation");
        let mut correlation = closed_correlation(&run_id);
        correlation.attempt_id.clear();
        let error = record_event(
            &db,
            &correlation,
            BoundaryLayer::Generated,
            BoundaryEventKind::HandshakeStart,
            RecordCompleteness::Complete,
            DiscoveryLocation {
                module: "M04",
                component: "C11",
                tool_instance: None,
            },
            serde_json::json!({}),
        )
        .expect_err("incomplete correlation must be rejected");
        assert!(error.to_string().contains("broken_correlation"), "{error}");
    }

    #[test]
    fn missing_handshake_end_is_outcome_unknown_not_success() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-missing-end");
        let correlation = closed_correlation(&run_id);
        record_event(
            &db,
            &correlation,
            BoundaryLayer::Generated,
            BoundaryEventKind::HandshakeStart,
            RecordCompleteness::Complete,
            DiscoveryLocation {
                module: "M04",
                component: "C11",
                tool_instance: None,
            },
            serde_json::json!({"schemaVersion": 1}),
        )
        .expect("start");
        close_open_handshakes(&db, &run_id).expect("close");
        let completeness = assess_completeness(&db, &run_id).expect("assess");
        assert_eq!(completeness, RecordCompleteness::OutcomeUnknown);
        let events = query_by_run(&db, &run_id).expect("query");
        assert!(
            events
                .iter()
                .any(|event| event.event_kind == BoundaryEventKind::MissingEnd),
            "{events:?}"
        );
        assert!(
            !events.iter().any(|event| {
                event.event_kind == BoundaryEventKind::HandshakeEnd
                    && event.record_completeness == RecordCompleteness::Complete
            }),
            "missing end must not be inferred as a successful handshake"
        );
    }

    #[test]
    fn persist_failure_is_independent_health_not_empty_success() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-persist-fail");
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE audit_boundary_events")?;
            Ok(())
        })
        .expect("drop");
        let error = record_event(
            &db,
            &closed_correlation(&run_id),
            BoundaryLayer::Generated,
            BoundaryEventKind::HandshakeStart,
            RecordCompleteness::Complete,
            DiscoveryLocation {
                module: "M04",
                component: "C11",
                tool_instance: None,
            },
            serde_json::json!({}),
        )
        .expect_err("persist must fail");
        assert!(!error.to_string().is_empty());
        assert!(
            persist_failed(&run_id),
            "audit health must not rely on the same dropped table"
        );
        let query = query_by_run(&db, &run_id);
        assert!(query.is_err(), "query failure must not look like no errors");
        assert!(!query.as_ref().is_ok_and(Vec::is_empty));
    }

    #[test]
    fn loop_recovery_events_are_retained_past_twelve() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-loop-retention");
        let correlation = closed_correlation(&run_id);
        for index in 0..15 {
            record_loop_event(
                &db,
                &correlation,
                &serde_json::json!({"event": "repair", "round": index}),
            )
            .expect("loop event");
        }
        let events = query_by_run(&db, &run_id).expect("query");
        let repairs = events
            .iter()
            .filter(|event| event.event_kind == BoundaryEventKind::LoopEvent)
            .count();
        assert_eq!(repairs, 15, "middle recovery events must not be dropped");
    }

    #[test]
    fn four_layers_are_distinct_records() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-four-layers");
        let correlation = closed_correlation(&run_id);
        let slot = BoundaryAuditSlot::new(correlation.clone());
        slot.note_generated(&messages_with_secret_note(), 1, true);
        record_slot_layer(&db, &slot, BoundaryLayer::Generated).expect("generated");
        slot.note_serialized(
            ProtocolFamily::OpenAiChatCompletions,
            &serde_json::json!({
                "model": "contract-model",
                "messages": [{"role": "tool"}],
                "tools": [{}]
            }),
            &messages_with_secret_note(),
        );
        record_slot_layer(&db, &slot, BoundaryLayer::Serialized).expect("serialized");
        slot.note_request_sent();
        record_slot_layer(&db, &slot, BoundaryLayer::RequestSent).expect("sent");
        slot.note_provider_returned(ProviderReturnStructure {
            http_status_class: Some("2xx"),
            has_content: true,
            tool_call_count: 0,
            finish_reason_class: Some("stop"),
        });
        record_slot_layer(&db, &slot, BoundaryLayer::ProviderReturned).expect("returned");
        slot.note_handshake_end(RecordCompleteness::Complete);
        record_slot_layer(&db, &slot, BoundaryLayer::ProviderReturned).expect("end layer ignored");
        record_handshake_end(&db, &slot).expect("end");
        let events = query_by_run(&db, &run_id).expect("query");
        let layers: Vec<_> = events.iter().map(|event| event.layer).collect();
        assert!(layers.contains(&BoundaryLayer::Generated));
        assert!(layers.contains(&BoundaryLayer::Serialized));
        assert!(layers.contains(&BoundaryLayer::RequestSent));
        assert!(layers.contains(&BoundaryLayer::ProviderReturned));
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_kind == BoundaryEventKind::HandshakeEnd)
                .count(),
            1
        );
        let joined = serde_json::to_string(&events).expect("json");
        assert!(!joined.contains(SECRET_NOTE), "{joined}");
    }

    #[test]
    fn known_abort_is_recorded_complete_not_missing_end() {
        let db = Database::open_in_memory().expect("db");
        let run_id = accept_run(&db, "c26-known-abort");
        let slot = BoundaryAuditSlot::new(closed_correlation(&run_id));
        slot.note_generated(&messages_with_secret_note(), 0, true);
        record_handshake_start(&db, &slot).expect("start");
        {
            let _guard = BoundaryPersistGuard::new(&db, slot.clone());
            slot.note_known_failure("aborted");
        }
        let events = query_by_run(&db, &run_id).expect("query");
        assert!(
            events
                .iter()
                .any(|event| event.event_kind == BoundaryEventKind::HandshakeEnd
                    && event.record_completeness == RecordCompleteness::Complete),
            "{events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|event| event.event_kind == BoundaryEventKind::MissingEnd),
            "a known cancel must not be stored as a missing handshake end: {events:?}"
        );
        assert_eq!(
            assess_completeness(&db, &run_id).expect("assess"),
            RecordCompleteness::Complete
        );
    }
}
