//! C27 诊断查询与解释：读取 C26 事件，给出已知事实、直接失败、恢复结果与待证根因。
//!
//! 本模块不保存第二套运行状态，不通过猜测代替事实。查询失败不得伪装成
//! 「没有发现问题」。`recovery_exhausted` 是恢复结果，不是根因。

use serde::{Deserialize, Serialize};

use super::agent_run_repository::AgentRunRepository;
use super::boundary_events::{
    persist_failed, query_by_run, BoundaryEventKind, BoundaryEventRecord, BoundaryLayer,
    RecordCompleteness,
};
use super::run_contract::{AssistantSessionRef, SafeRunErrorCode, SecurityDomain};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Attribution state. Only moves toward more evidence; never compresses a Run
/// into a single first-error field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributionStatus {
    Unattributed,
    Suspected,
    Confirmed,
    MultipleCauses,
}

/// Mechanical issue class. Semantic quality is never confirmed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueClass {
    InternalContract,
    ExternalService,
    ToolExecution,
    ModelBehavior,
    ExpectedRestriction,
    DiagnosticGap,
}

/// Independent audit-channel health. Must not share the failed log path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditHealth {
    pub persist_failed: bool,
}

/// Discovery location. This is never the confirmed root-cause field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveryRef {
    pub module: String,
    pub component: String,
    pub tool_instance: Option<String>,
    pub call_id: String,
    pub attempt_id: String,
    pub model_turn: u32,
}

/// One explained fact. `confirmed_source` is absent unless mechanically proven.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticFinding {
    pub statement: String,
    pub discovery: DiscoveryRef,
    pub issue_class: IssueClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_source: Option<String>,
}

/// C27 query result. Query infrastructure failure is `AppResult::Err`, not this shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticReport {
    pub schema_version: u8,
    pub run_id: String,
    pub input_revision: Option<String>,
    pub parent_run_id: Option<String>,
    pub child_run_id: Option<String>,
    pub record_completeness: RecordCompleteness,
    pub attribution_status: AttributionStatus,
    pub audit_health: AuditHealth,
    pub headline: String,
    pub impact: String,
    pub recovery_state: String,
    pub known_facts: Vec<DiagnosticFinding>,
    pub direct_failures: Vec<DiagnosticFinding>,
    pub recovery_results: Vec<DiagnosticFinding>,
    pub pending_root_causes: Vec<DiagnosticFinding>,
    pub expected_restrictions: Vec<DiagnosticFinding>,
    pub evidence_gaps: Vec<DiagnosticFinding>,
    pub path: Vec<DiscoveryRef>,
}

fn discovery_of(event: &BoundaryEventRecord) -> DiscoveryRef {
    DiscoveryRef {
        module: event.module_id.clone(),
        component: event.component_id.clone(),
        tool_instance: event.tool_instance.clone(),
        call_id: event.call_id.clone(),
        attempt_id: event.attempt_id.clone(),
        model_turn: event.model_turn,
    }
}

fn finding(
    event: &BoundaryEventRecord,
    statement: impl Into<String>,
    issue_class: IssueClass,
    confirmed_source: Option<String>,
) -> DiagnosticFinding {
    DiagnosticFinding {
        statement: statement.into(),
        discovery: discovery_of(event),
        issue_class,
        confirmed_source,
    }
}

fn payload_event(event: &BoundaryEventRecord) -> Option<&str> {
    event
        .payload
        .get("event")
        .and_then(serde_json::Value::as_str)
}

fn payload_reason(event: &BoundaryEventRecord) -> Option<&str> {
    event
        .payload
        .get("reason")
        .and_then(serde_json::Value::as_str)
}

fn payload_tool(event: &BoundaryEventRecord) -> Option<&str> {
    event.tool_instance.as_deref().or_else(|| {
        event
            .payload
            .get("tool")
            .and_then(serde_json::Value::as_str)
    })
}

fn payload_flag(event: &BoundaryEventRecord, key: &str) -> bool {
    event
        .payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn synthetic_gap(statement: &str) -> DiagnosticFinding {
    DiagnosticFinding {
        statement: statement.to_string(),
        discovery: DiscoveryRef {
            module: "M09".into(),
            component: "C27".into(),
            tool_instance: None,
            call_id: "query".into(),
            attempt_id: "query".into(),
            model_turn: 0,
        },
        issue_class: IssueClass::DiagnosticGap,
        confirmed_source: None,
    }
}

fn completeness_from(events: &[BoundaryEventRecord], persist_failed: bool) -> RecordCompleteness {
    if persist_failed {
        return RecordCompleteness::PersistFailed;
    }
    if events.is_empty() {
        return RecordCompleteness::MissingEvents;
    }
    if events
        .iter()
        .any(|event| event.record_completeness == RecordCompleteness::PersistFailed)
    {
        return RecordCompleteness::PersistFailed;
    }
    if events
        .iter()
        .any(|event| event.record_completeness == RecordCompleteness::BrokenCorrelation)
    {
        return RecordCompleteness::BrokenCorrelation;
    }
    if events.iter().any(|event| {
        event.event_kind == BoundaryEventKind::MissingEnd
            || event.record_completeness == RecordCompleteness::OutcomeUnknown
    }) {
        return RecordCompleteness::OutcomeUnknown;
    }
    if events
        .iter()
        .any(|event| event.record_completeness == RecordCompleteness::MissingEvents)
    {
        return RecordCompleteness::MissingEvents;
    }
    RecordCompleteness::Complete
}

/// Interpret already-loaded C26 events. Empty events are a gap, never a pass.
pub(crate) fn interpret_events(
    run_id: &str,
    events: &[BoundaryEventRecord],
    persist_failed: bool,
) -> DiagnosticReport {
    let mut known_facts = Vec::new();
    let mut direct_failures = Vec::new();
    let mut recovery_results = Vec::new();
    let mut pending_root_causes = Vec::new();
    let mut expected_restrictions = Vec::new();
    let mut evidence_gaps = Vec::new();
    let mut path: Vec<DiscoveryRef> = Vec::new();
    let mut layers_by_attempt: std::collections::BTreeMap<String, Vec<BoundaryLayer>> =
        std::collections::BTreeMap::new();

    for event in events {
        let discovery = discovery_of(event);
        if !path.iter().any(|step| {
            step.module == discovery.module
                && step.component == discovery.component
                && step.call_id == discovery.call_id
                && step.attempt_id == discovery.attempt_id
        }) {
            path.push(discovery);
        }
        layers_by_attempt
            .entry(event.attempt_id.clone())
            .or_default()
            .push(event.layer);

        match event.event_kind {
            BoundaryEventKind::HandshakeStart => {
                known_facts.push(finding(
                    event,
                    "握手开始已记录",
                    IssueClass::InternalContract,
                    None,
                ));
            }
            BoundaryEventKind::HandshakeEnd => {
                known_facts.push(finding(
                    event,
                    "握手结束已记录",
                    IssueClass::InternalContract,
                    None,
                ));
            }
            BoundaryEventKind::MissingEnd => {
                evidence_gaps.push(finding(
                    event,
                    "握手缺少结束事件，结果未知",
                    IssueClass::DiagnosticGap,
                    None,
                ));
            }
            BoundaryEventKind::PersistFailed => {
                evidence_gaps.push(finding(
                    event,
                    "审计写入失败",
                    IssueClass::DiagnosticGap,
                    None,
                ));
            }
            BoundaryEventKind::OutboundWitness => {
                known_facts.push(finding(
                    event,
                    format!("出站层 {} 已记录", event.layer.as_str()),
                    IssueClass::InternalContract,
                    None,
                ));
            }
            BoundaryEventKind::LoopEvent => match payload_event(event) {
                Some("proposal") => {
                    let tool = payload_tool(event).unwrap_or("unknown");
                    let catalog_known = payload_flag(event, "catalogKnown");
                    known_facts.push(finding(
                        event,
                        format!("工具提议 {tool} 在派发边界被记录"),
                        IssueClass::ToolExecution,
                        None,
                    ));
                    if !catalog_known {
                        pending_root_causes.push(finding(
                            event,
                            format!("工具名 {tool} 的来源未区分，不能证实为模型故障"),
                            IssueClass::ModelBehavior,
                            None,
                        ));
                    }
                }
                Some("repair") => {
                    recovery_results.push(finding(
                        event,
                        "已记录一次修复尝试",
                        IssueClass::InternalContract,
                        None,
                    ));
                }
                Some("tool_error") => {
                    let tool = payload_tool(event).unwrap_or("tool");
                    direct_failures.push(finding(
                        event,
                        format!("工具 {tool} 执行失败"),
                        IssueClass::ToolExecution,
                        Some("tool-execution".into()),
                    ));
                }
                Some("result")
                    if event
                        .payload
                        .get("success")
                        .and_then(serde_json::Value::as_bool)
                        == Some(false) =>
                {
                    let tool = payload_tool(event).unwrap_or("tool");
                    direct_failures.push(finding(
                        event,
                        format!("工具 {tool} 执行失败"),
                        IssueClass::ToolExecution,
                        Some("tool-execution".into()),
                    ));
                }
                Some("exit") => {
                    let reason = payload_reason(event).unwrap_or("unknown");
                    if reason == "recovery_exhausted" {
                        recovery_results.push(finding(
                            event,
                            "恢复耗尽，这是恢复结果而不是根因",
                            IssueClass::InternalContract,
                            None,
                        ));
                    } else if reason == "tool_unavailable"
                        || payload_flag(event, "capabilityBlocked")
                    {
                        expected_restrictions.push(finding(
                            event,
                            "能力不可用或权限限制，属预期限制",
                            IssueClass::ExpectedRestriction,
                            None,
                        ));
                    } else if matches!(
                        reason,
                        "cancelled" | "agent_run_cancelled" | "user_cancelled"
                    ) {
                        expected_restrictions.push(finding(
                            event,
                            "用户取消，属预期限制",
                            IssueClass::ExpectedRestriction,
                            None,
                        ));
                    } else {
                        known_facts.push(finding(
                            event,
                            format!("循环退出原因 {reason}"),
                            IssueClass::InternalContract,
                            None,
                        ));
                    }
                }
                Some(other) => {
                    known_facts.push(finding(
                        event,
                        format!("循环事件 {other}"),
                        IssueClass::InternalContract,
                        None,
                    ));
                }
                None => {
                    known_facts.push(finding(
                        event,
                        "循环事件已记录",
                        IssueClass::InternalContract,
                        None,
                    ));
                }
            },
        }
    }

    for (attempt_id, layers) in layers_by_attempt {
        let has_generated = layers.contains(&BoundaryLayer::Generated)
            && events.iter().any(|event| {
                event.attempt_id == attempt_id
                    && event.event_kind == BoundaryEventKind::OutboundWitness
                    && event.layer == BoundaryLayer::Generated
            });
        let has_request_sent = layers.contains(&BoundaryLayer::RequestSent);
        if has_generated && !has_request_sent {
            if let Some(event) = events.iter().find(|event| event.attempt_id == attempt_id) {
                evidence_gaps.push(finding(
                    event,
                    "已生成出站结构，但未记录 request_sent，不能证明请求已发送",
                    IssueClass::DiagnosticGap,
                    None,
                ));
            }
        }
    }

    if persist_failed {
        evidence_gaps.push(synthetic_gap("审计写入失败，诊断通道可能不完整"));
    }
    if events.is_empty() && !persist_failed {
        evidence_gaps.push(synthetic_gap("没有可关联的边界事件，不能当作执行成功"));
    }

    let record_completeness = completeness_from(events, persist_failed);
    let independent_failures = direct_failures.len();
    let attribution_status = if record_completeness != RecordCompleteness::Complete
        || !pending_root_causes.is_empty()
        || !evidence_gaps.is_empty()
    {
        if independent_failures >= 2 {
            AttributionStatus::MultipleCauses
        } else {
            AttributionStatus::Unattributed
        }
    } else if independent_failures >= 2 {
        AttributionStatus::MultipleCauses
    } else if independent_failures == 1 && pending_root_causes.is_empty() {
        AttributionStatus::Confirmed
    } else {
        AttributionStatus::Unattributed
    };

    let recovered = !direct_failures.is_empty() && !recovery_results.is_empty();
    let (headline, impact, recovery_state) = if persist_failed {
        (
            "审计写入失败，诊断可能不完整。".to_string(),
            "不能仅凭缺失记录推断任务成功。".to_string(),
            "审计健康通道已标记失败。".to_string(),
        )
    } else if events.is_empty() {
        (
            "记录不完整，无法确认本次执行结果。".to_string(),
            "缺少边界事件，不能评价迁移或质量。".to_string(),
            "未知。".to_string(),
        )
    } else if record_completeness == RecordCompleteness::OutcomeUnknown {
        (
            "握手未结束，结果未知。".to_string(),
            "不能把缺失结束当成成功。".to_string(),
            "未知。".to_string(),
        )
    } else if attribution_status == AttributionStatus::MultipleCauses {
        (
            "存在多个独立问题，不能压成单一首错。".to_string(),
            "各失败需分别查看。".to_string(),
            if recovered {
                "部分恢复已记录，原始失败仍可查看。".to_string()
            } else {
                "未证实全部恢复。".to_string()
            },
        )
    } else if !direct_failures.is_empty()
        && recovery_results
            .iter()
            .any(|item| item.statement.contains("恢复耗尽"))
    {
        (
            "局部失败，恢复已耗尽；原始失败仍可查看。".to_string(),
            "恢复耗尽不是根因。".to_string(),
            "恢复耗尽。".to_string(),
        )
    } else if !expected_restrictions.is_empty() && direct_failures.is_empty() {
        (
            "预期限制，不是内部故障。".to_string(),
            "正常拒绝或能力限制。".to_string(),
            "按限制结束。".to_string(),
        )
    } else if recovered {
        (
            "已恢复，原始失败仍可查看。".to_string(),
            "不得因恢复抹去原始失败。".to_string(),
            "已记录修复尝试。".to_string(),
        )
    } else if record_completeness == RecordCompleteness::Complete && direct_failures.is_empty() {
        (
            "未发现可证实的故障。".to_string(),
            "仅覆盖已记录的机械事实。".to_string(),
            "无恢复轨迹。".to_string(),
        )
    } else {
        (
            "诊断不完整，尚不能证实根因。".to_string(),
            "待证项见缺口与待证根因。".to_string(),
            "未知。".to_string(),
        )
    };

    let first = events.first();
    DiagnosticReport {
        schema_version: 1,
        run_id: run_id.to_string(),
        input_revision: first.map(|event| event.input_revision.clone()),
        parent_run_id: first.and_then(|event| event.parent_run_id.clone()),
        child_run_id: first.and_then(|event| event.child_run_id.clone()),
        record_completeness,
        attribution_status,
        audit_health: AuditHealth { persist_failed },
        headline,
        impact,
        recovery_state,
        known_facts,
        direct_failures,
        recovery_results,
        pending_root_causes,
        expected_restrictions,
        evidence_gaps,
        path,
    }
}

/// Diagnose one Run bound to a session. Empty `run_id` is an error, not an empty pass.
pub(crate) fn diagnose_run(
    db: &Database,
    session: &AssistantSessionRef,
    run_id: &str,
) -> AppResult<DiagnosticReport> {
    let run_id = run_id.trim();
    if run_id.is_empty() {
        return Err(AppError::run(SafeRunErrorCode::InvalidRequest));
    }
    if session.domain == SecurityDomain::Classified {
        let mut report = interpret_events(run_id, &[], false);
        report.evidence_gaps.insert(
            0,
            synthetic_gap("涉密 Run 不持久化诊断记录，不能从磁盘查询推断没有问题"),
        );
        report.headline = "涉密运行仅进程内可诊断，持久记录缺失。".into();
        report.impact = "无法用历史回放评价本次执行。".into();
        report.recovery_state = "未知。".into();
        report.attribution_status = AttributionStatus::Unattributed;
        report.record_completeness = RecordCompleteness::MissingEvents;
        return Ok(report);
    }
    let found = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?;
    if found.is_none() {
        return Err(AppError::run(SafeRunErrorCode::RunNotFound));
    }
    let persist_failed = persist_failed(run_id);
    match query_by_run(db, run_id) {
        Ok(events) => Ok(interpret_events(run_id, &events, persist_failed)),
        Err(_) if persist_failed => {
            let mut report = interpret_events(run_id, &[], true);
            report
                .evidence_gaps
                .push(synthetic_gap("诊断查询在审计写入失败后无法读取事件"));
            report.headline = "审计写入失败，诊断可能不完整。".into();
            Ok(report)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
    use crate::ai_runtime::boundary_events::{
        persist_failed, record_event, record_loop_event, BoundaryCorrelation, BoundaryEventKind,
        BoundaryLayer, DiscoveryLocation,
    };
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::run_contract::{
        ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality,
        RiskClass, SafeRunErrorCode, SecurityDomain, WebDecisionReason,
    };

    fn discovery(event: &BoundaryEventRecord) -> DiscoveryRef {
        DiscoveryRef {
            module: event.module_id.clone(),
            component: event.component_id.clone(),
            tool_instance: event.tool_instance.clone(),
            call_id: event.call_id.clone(),
            attempt_id: event.attempt_id.clone(),
            model_turn: event.model_turn,
        }
    }

    fn record(
        kind: BoundaryEventKind,
        layer: BoundaryLayer,
        completeness: RecordCompleteness,
        payload: serde_json::Value,
    ) -> BoundaryEventRecord {
        BoundaryEventRecord {
            run_id: "run-1".into(),
            input_revision: "rev-1".into(),
            parent_run_id: None,
            child_run_id: None,
            model_turn: 1,
            call_id: "call-1".into(),
            attempt_id: "attempt-1".into(),
            tool_surface_version: "surface-v1".into(),
            protocol_adapter: "openai_chat_completions".into(),
            layer,
            event_kind: kind,
            record_completeness: completeness,
            module_id: "M05".into(),
            component_id: "C14".into(),
            tool_instance: payload
                .get("tool")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            payload,
        }
    }

    fn accept_run(db: &Database, run_id: &str) -> (AssistantSessionRef, String) {
        let session = NormalSessionRepository::create(db).expect("session");
        let accepted = AgentRunRepository::accept(
            db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key.clone(),
                client_request_id: format!("{run_id}-client"),
                run_id: run_id.to_string(),
                turn_id: format!("{run_id}-turn"),
                message: "diagnose".into(),
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
        .expect("accepted");
        (
            AssistantSessionRef {
                domain: SecurityDomain::Normal,
                session_key: session.session_key,
            },
            accepted.run_id,
        )
    }

    #[test]
    fn empty_events_are_missing_events_not_a_pass() {
        let report = interpret_events("run-1", &[], false);
        assert_eq!(
            report.record_completeness,
            RecordCompleteness::MissingEvents
        );
        assert_eq!(report.attribution_status, AttributionStatus::Unattributed);
        assert!(!report.audit_health.persist_failed);
        assert!(!report.evidence_gaps.is_empty());
        assert!(
            !report.headline.contains("未发现问题"),
            "empty records must not look like a clean bill of health: {}",
            report.headline
        );
        assert!(
            report.direct_failures.is_empty(),
            "missing records are a gap, not a fabricated failure"
        );
    }

    #[test]
    fn persist_failed_is_independent_health_and_a_gap() {
        let report = interpret_events("run-1", &[], true);
        assert!(report.audit_health.persist_failed);
        assert_eq!(
            report.record_completeness,
            RecordCompleteness::PersistFailed
        );
        assert!(report.evidence_gaps.iter().any(|gap| {
            gap.issue_class == IssueClass::DiagnosticGap && gap.statement.contains("审计写入失败")
        }));
        assert_ne!(report.attribution_status, AttributionStatus::Confirmed);
    }

    #[test]
    fn recovery_exhausted_is_recovery_not_confirmed_root_cause() {
        let failure = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"tool_error","tool":"web_fetch","reason":"provider_timeout"}),
        );
        let repair = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"repair","round":1}),
        );
        let exit = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"exit","reason":"recovery_exhausted"}),
        );
        let report = interpret_events("run-1", &[failure, repair, exit], false);
        assert!(
            report
                .direct_failures
                .iter()
                .any(|item| item.statement.contains("web_fetch")),
            "original failure must remain: {:?}",
            report.direct_failures
        );
        assert!(
            report.recovery_results.iter().any(|item| {
                item.statement.contains("恢复耗尽") || item.statement.contains("recovery")
            }),
            "{:?}",
            report.recovery_results
        );
        assert!(
            !report.direct_failures.iter().any(|item| {
                item.confirmed_source
                    .as_deref()
                    .is_some_and(|source| source.contains("recovery_exhausted"))
                    || item.statement.contains("恢复耗尽")
            }),
            "recovery exhaustion must not be stored as the confirmed failure: {:?}",
            report.direct_failures
        );
        assert!(
            !report
                .pending_root_causes
                .iter()
                .any(|item| { item.confirmed_source.as_deref() == Some("recovery_exhausted") }),
            "{:?}",
            report.pending_root_causes
        );
    }

    #[test]
    fn missing_handshake_end_is_outcome_unknown_not_success() {
        let start = BoundaryEventRecord {
            event_kind: BoundaryEventKind::HandshakeStart,
            layer: BoundaryLayer::Generated,
            module_id: "M04".into(),
            component_id: "C11".into(),
            ..record(
                BoundaryEventKind::HandshakeStart,
                BoundaryLayer::Generated,
                RecordCompleteness::Complete,
                serde_json::json!({"schemaVersion":1}),
            )
        };
        let missing = BoundaryEventRecord {
            event_kind: BoundaryEventKind::MissingEnd,
            layer: BoundaryLayer::ProviderReturned,
            record_completeness: RecordCompleteness::OutcomeUnknown,
            module_id: "M09".into(),
            component_id: "C26".into(),
            ..record(
                BoundaryEventKind::MissingEnd,
                BoundaryLayer::ProviderReturned,
                RecordCompleteness::OutcomeUnknown,
                serde_json::json!({"schemaVersion":1}),
            )
        };
        let report = interpret_events("run-1", &[start, missing], false);
        assert_eq!(
            report.record_completeness,
            RecordCompleteness::OutcomeUnknown
        );
        assert!(report
            .evidence_gaps
            .iter()
            .any(|gap| gap.issue_class == IssueClass::DiagnosticGap));
        assert_ne!(report.attribution_status, AttributionStatus::Confirmed);
        assert!(!report.headline.contains("未发现问题"));
    }

    #[test]
    fn four_outbound_layers_are_separate_facts() {
        let mut events = Vec::new();
        for (layer, kind) in [
            (BoundaryLayer::Generated, BoundaryEventKind::OutboundWitness),
            (
                BoundaryLayer::Serialized,
                BoundaryEventKind::OutboundWitness,
            ),
            (
                BoundaryLayer::RequestSent,
                BoundaryEventKind::OutboundWitness,
            ),
            (
                BoundaryLayer::ProviderReturned,
                BoundaryEventKind::OutboundWitness,
            ),
        ] {
            events.push(BoundaryEventRecord {
                layer,
                event_kind: kind,
                module_id: "M04".into(),
                component_id: "C11".into(),
                payload: serde_json::json!({
                    "schemaVersion": 1,
                    "protocolFamily": "openai_chat_completions",
                    "repairIncluded": true
                }),
                ..record(
                    kind,
                    layer,
                    RecordCompleteness::Complete,
                    serde_json::json!({}),
                )
            });
        }
        let report = interpret_events("run-1", &events, false);
        let layers: Vec<_> = report
            .known_facts
            .iter()
            .map(|fact| fact.statement.as_str())
            .collect();
        assert!(
            layers.iter().any(|text| text.contains("generated")),
            "{layers:?}"
        );
        assert!(
            layers.iter().any(|text| text.contains("serialized")),
            "{layers:?}"
        );
        assert!(
            layers.iter().any(|text| text.contains("request_sent")),
            "{layers:?}"
        );
        assert!(
            layers.iter().any(|text| text.contains("provider_returned")),
            "{layers:?}"
        );
        assert!(
            report.evidence_gaps.is_empty(),
            "complete four-layer handshake must not invent a gap: {:?}",
            report.evidence_gaps
        );
    }

    #[test]
    fn generated_without_request_sent_is_a_gap() {
        let generated = BoundaryEventRecord {
            layer: BoundaryLayer::Generated,
            event_kind: BoundaryEventKind::OutboundWitness,
            module_id: "M04".into(),
            component_id: "C11".into(),
            ..record(
                BoundaryEventKind::OutboundWitness,
                BoundaryLayer::Generated,
                RecordCompleteness::Complete,
                serde_json::json!({"schemaVersion":1}),
            )
        };
        let report = interpret_events("run-1", &[generated], false);
        assert!(report.evidence_gaps.iter().any(|gap| {
            gap.statement.contains("request_sent") || gap.statement.contains("未发送")
        }));
    }

    #[test]
    fn two_independent_tool_failures_are_multiple_causes() {
        let fetch = BoundaryEventRecord {
            call_id: "call-fetch".into(),
            attempt_id: "attempt-fetch".into(),
            tool_instance: Some("web_fetch".into()),
            ..record(
                BoundaryEventKind::LoopEvent,
                BoundaryLayer::Generated,
                RecordCompleteness::Complete,
                serde_json::json!({"event":"tool_error","tool":"web_fetch"}),
            )
        };
        let search = BoundaryEventRecord {
            call_id: "call-search".into(),
            attempt_id: "attempt-search".into(),
            tool_instance: Some("web_search".into()),
            ..record(
                BoundaryEventKind::LoopEvent,
                BoundaryLayer::Generated,
                RecordCompleteness::Complete,
                serde_json::json!({"event":"tool_error","tool":"web_search"}),
            )
        };
        let report = interpret_events("run-1", &[fetch, search], false);
        assert_eq!(report.attribution_status, AttributionStatus::MultipleCauses);
        assert_eq!(report.direct_failures.len(), 2);
        assert!(report.headline.contains("多个") || report.headline.contains("独立"));
    }

    #[test]
    fn capability_blocked_is_expected_restriction_not_a_bug() {
        let exit = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"exit","reason":"tool_unavailable","capabilityBlocked":true}),
        );
        let report = interpret_events("run-1", &[exit], false);
        assert!(report
            .expected_restrictions
            .iter()
            .any(|item| item.issue_class == IssueClass::ExpectedRestriction));
        assert!(report.direct_failures.is_empty());
        assert!(!report.headline.contains("程序错误"));
    }

    #[test]
    fn unknown_tool_proposal_is_not_confirmed_model_fault() {
        let proposal = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"proposal","tool":"unknown_search","catalogKnown":false,"reason":"unknown_tool"}),
        );
        let report = interpret_events("run-1", &[proposal], false);
        assert_eq!(report.attribution_status, AttributionStatus::Unattributed);
        assert!(report.pending_root_causes.iter().any(|item| {
            item.issue_class == IssueClass::ModelBehavior
                && item.confirmed_source.is_none()
                && item.discovery.component == "C14"
        }));
        assert!(!report
            .direct_failures
            .iter()
            .any(|item| item.confirmed_source.as_deref() == Some("model")));
    }

    #[test]
    fn path_lists_module_component_and_call() {
        let failure = record(
            BoundaryEventKind::LoopEvent,
            BoundaryLayer::Generated,
            RecordCompleteness::Complete,
            serde_json::json!({"event":"tool_error","tool":"web_fetch"}),
        );
        let report = interpret_events("run-1", std::slice::from_ref(&failure), false);
        assert!(
            report.path.iter().any(|step| {
                step.module == "M05" && step.component == "C14" && step.call_id == "call-1"
            }),
            "{:?}",
            report.path
        );
        assert_eq!(report.path[0], discovery(&failure));
    }

    #[test]
    fn diagnose_run_rejects_empty_run_id() {
        let db = Database::open_in_memory().expect("db");
        let session = AssistantSessionRef {
            domain: SecurityDomain::Normal,
            session_key: "session".into(),
        };
        let error = diagnose_run(&db, &session, "  ").expect_err("empty id");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&error),
            SafeRunErrorCode::InvalidRequest
        );
    }

    #[test]
    fn diagnose_run_classified_reports_gap_not_empty_success() {
        let db = Database::open_in_memory().expect("db");
        let session = AssistantSessionRef {
            domain: SecurityDomain::Classified,
            session_key: "classified".into(),
        };
        let report = diagnose_run(&db, &session, "classified-run").expect("classified");
        assert_eq!(report.run_id, "classified-run");
        assert_eq!(
            report.record_completeness,
            RecordCompleteness::MissingEvents
        );
        assert!(report
            .evidence_gaps
            .iter()
            .any(|gap| gap.issue_class == IssueClass::DiagnosticGap));
        assert!(!report.headline.contains("未发现问题"));
    }

    #[test]
    fn diagnose_run_unknown_run_is_error_not_empty_pass() {
        let db = Database::open_in_memory().expect("db");
        let (session, _) = accept_run(&db, "c27-known");
        let error = diagnose_run(&db, &session, "missing-run").expect_err("unknown");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&error),
            SafeRunErrorCode::RunNotFound
        );
    }

    #[test]
    fn diagnose_run_query_failure_without_persist_flag_is_error() {
        let db = Database::open_in_memory().expect("db");
        let (session, run_id) = accept_run(&db, "c27-query-fail");
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE audit_boundary_events")?;
            Ok(())
        })
        .expect("drop");
        let error = diagnose_run(&db, &session, &run_id).expect_err("query fail");
        assert_eq!(
            SafeRunErrorCode::from_app_error(&error),
            SafeRunErrorCode::PersistenceFailed
        );
    }

    #[test]
    fn diagnose_run_persist_failure_still_returns_health_report() {
        let db = Database::open_in_memory().expect("db");
        let (session, run_id) = accept_run(&db, "c27-persist-fail");
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE audit_boundary_events")?;
            Ok(())
        })
        .expect("drop");
        let _ = record_event(
            &db,
            &BoundaryCorrelation {
                run_id: run_id.clone(),
                input_revision: "rev-1".into(),
                parent_run_id: None,
                child_run_id: None,
                model_turn: 1,
                call_id: "call-1".into(),
                attempt_id: "attempt-1".into(),
                tool_surface_version: "surface-v1".into(),
                protocol_adapter: "openai_chat_completions".into(),
            },
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
        assert!(persist_failed(&run_id));
        let report = diagnose_run(&db, &session, &run_id).expect("health report");
        assert!(report.audit_health.persist_failed);
        assert_eq!(
            report.record_completeness,
            RecordCompleteness::PersistFailed
        );
        assert!(!report.headline.contains("未发现问题"));
    }

    #[test]
    fn diagnose_run_reads_c26_loop_events_not_the_twelve_item_snapshot() {
        let db = Database::open_in_memory().expect("db");
        let (session, run_id) = accept_run(&db, "c27-loop");
        let correlation = BoundaryCorrelation {
            run_id: run_id.clone(),
            input_revision: "rev-1".into(),
            parent_run_id: None,
            child_run_id: None,
            model_turn: 1,
            call_id: "loop".into(),
            attempt_id: "loop-1".into(),
            tool_surface_version: "surface-v1".into(),
            protocol_adapter: "tool_loop".into(),
        };
        for round in 1..=15 {
            record_loop_event(
                &db,
                &correlation,
                &serde_json::json!({"event":"repair","round":round}),
            )
            .expect("loop");
        }
        record_loop_event(
            &db,
            &correlation,
            &serde_json::json!({"event":"exit","reason":"recovery_exhausted"}),
        )
        .expect("exit");
        let report = diagnose_run(&db, &session, &run_id).expect("diagnose");
        assert!(
            report.recovery_results.len() >= 15,
            "C27 must read the append-only C26 table, not the 12-event snapshot: {}",
            report.recovery_results.len()
        );
    }
}
