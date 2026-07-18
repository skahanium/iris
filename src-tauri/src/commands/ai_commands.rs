//! Non-execution AI configuration and diagnostic IPC commands.
//!
//! Agent execution is exposed exclusively through `assistant_run_*`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
#[derive(Debug, Clone, Default, Serialize)]
pub struct KnowledgeReindexResponse {
    pub anchors: usize,
    pub regulations: usize,
}
/// Re-index all knowledge: anchors, regulations, block links.
#[tauri::command]
pub async fn knowledge_reindex(
    state: State<'_, Arc<AppState>>,
) -> AppResult<KnowledgeReindexResponse> {
    let vault = state.vault_path()?;
    let mut stats = KnowledgeReindexResponse::default();

    state.db.with_conn(|conn| {
        // Re-index regulations
        match crate::knowledge::regulations::reindex_all_regulations(conn, &vault) {
            Ok(count) => {
                stats.regulations = count;
            }
            Err(e) => tracing::warn!("regulation reindex error: {e}"),
        }
        Ok::<_, crate::error::AppError>(())
    })?;
    crate::llm::safe_lock(&state.ai.context_cache).clear();

    Ok(stats)
}

/// List installed skills (global + vault) with validation status.
#[tauri::command]
pub async fn skills_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<crate::ai_runtime::skills::SkillListEntry>> {
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::list_skills(&state.db, &vault)
}

#[derive(Debug, Serialize)]
pub struct SkillsPathsResponse {
    pub global: String,
    pub vault: String,
}

/// Return resolved global and vault skill installation directories.
#[tauri::command]
pub async fn skills_paths(state: State<'_, Arc<AppState>>) -> AppResult<SkillsPathsResponse> {
    use crate::ai_runtime::skills::{global_skills_dir, vault_skills_dir};

    let vault = state.vault_path()?;
    Ok(SkillsPathsResponse {
        global: global_skills_dir().to_string_lossy().into_owned(),
        vault: vault_skills_dir(&vault).to_string_lossy().into_owned(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderInput {
    pub id: String,
    pub name: String,
    pub provider_kind: String,
    pub enabled: bool,
    pub transport_kind: Option<String>,
    #[serde(default = "default_provider_config_json")]
    pub transport_config_json: String,
    #[serde(default = "default_provider_config_json")]
    pub credential_refs_json: String,
    pub search_mapping: Option<String>,
    pub fetch_mapping: Option<String>,
}

fn default_provider_config_json() -> String {
    "{}".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderSummary {
    pub id: String,
    pub name: String,
    pub provider_kind: String,
    pub enabled: bool,
    pub transport_kind: String,
    pub transport_config_json: String,
    pub credential_refs_json: String,
    pub search_mapping: Option<String>,
    pub fetch_mapping: Option<String>,
    pub mapping_status: String,
    pub diagnostic_status: String,
    pub is_native: bool,
    pub editable: bool,
    pub has_search_mapping: bool,
    pub has_fetch_mapping: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderDiagnosticCheck {
    pub label: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEvidenceProviderDiagnostics {
    pub provider_id: Option<String>,
    pub is_runtime_selected: bool,
    pub status: String,
    pub failures: Vec<String>,
    pub checks: Vec<WebEvidenceProviderDiagnosticCheck>,
    pub can_use_for_search: bool,
    pub can_use_for_fetch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftScopeRuleDto {
    pub kind: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCreateDraftRequest {
    pub name: String,
    pub description: Option<String>,
    pub body: Option<String>,
    pub scope_rules: Vec<SkillDraftScopeRuleDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftDto {
    pub name: String,
    pub markdown: String,
    pub scope_rules: Vec<SkillDraftScopeRuleDto>,
    pub content_hash: String,
    pub target_path: String,
}

#[tauri::command]
pub async fn web_evidence_provider_upsert(
    state: State<'_, Arc<AppState>>,
    input: WebEvidenceProviderInput,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &state.db,
        &provider_input_to_registry(input)?,
    )
}

#[tauri::command]
pub async fn web_evidence_providers_list(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<WebEvidenceProviderSummary>> {
    crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&state.db).map(|items| {
        items
            .into_iter()
            .map(|item| {
                let mapping_status =
                    provider_mapping_status(item.has_search_mapping, item.has_fetch_mapping);
                WebEvidenceProviderSummary {
                    id: item.id,
                    name: item.name,
                    provider_kind: item.kind.clone(),
                    enabled: item.enabled,
                    transport_kind: item.transport_kind,
                    transport_config_json: item.transport_config_json,
                    credential_refs_json: item.credential_refs_json,
                    search_mapping: item.web_search_mapping_json,
                    fetch_mapping: item.web_fetch_mapping_json,
                    diagnostic_status: provider_diagnostic_status(item.enabled, &mapping_status),
                    mapping_status,
                    is_native: item.kind == "native",
                    editable: item.kind == "mcp",
                    has_search_mapping: item.has_search_mapping,
                    has_fetch_mapping: item.has_fetch_mapping,
                }
            })
            .collect()
    })
}

#[tauri::command]
pub async fn web_evidence_provider_toggle(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    enabled: bool,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::toggle_web_evidence_provider(
        &state.db,
        &provider_id,
        enabled,
    )
}

#[tauri::command]
pub async fn web_evidence_provider_delete(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<()> {
    crate::ai_runtime::mcp_runtime_registry::delete_web_evidence_provider(&state.db, &provider_id)
}

#[tauri::command]
pub async fn web_evidence_provider_diagnostics(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<WebEvidenceProviderDiagnostics> {
    let providers =
        crate::ai_runtime::mcp_runtime_registry::list_web_evidence_providers(&state.db)?;

    if let Some(provider) = providers.iter().find(|item| item.id == provider_id) {
        return provider_diagnostics_for_summary(&state.db, provider).await;
    }

    Ok(WebEvidenceProviderDiagnostics {
        provider_id: Some(provider_id),
        is_runtime_selected: false,
        status: "missing".into(),
        failures: vec!["未找到提供方记录".into()],
        checks: vec![provider_diagnostic_check(
            "configured",
            false,
            "提供方记录缺失",
        )],
        can_use_for_search: false,
        can_use_for_fetch: false,
    })
}

fn provider_mapping_status(has_search_mapping: bool, has_fetch_mapping: bool) -> String {
    match (has_search_mapping, has_fetch_mapping) {
        (true, true) => "complete".into(),
        (true, false) | (false, true) => "partial".into(),
        (false, false) => "missing".into(),
    }
}

fn provider_diagnostic_status(enabled: bool, mapping_status: &str) -> String {
    if !enabled {
        "disabled".into()
    } else if mapping_status == "complete" {
        "ready".into()
    } else {
        "needs_mapping".into()
    }
}

fn provider_diagnostic_check(
    label: &str,
    passed: bool,
    message: &str,
) -> WebEvidenceProviderDiagnosticCheck {
    WebEvidenceProviderDiagnosticCheck {
        label: label.into(),
        status: if passed { "pass" } else { "fail" }.into(),
        message: message.into(),
    }
}

fn provider_credential_service_from_binding(value: &serde_json::Value) -> Option<String> {
    let raw = if let Some(raw) = value.as_str() {
        raw.trim()
    } else if let Some(object) = value.as_object() {
        object
            .get("credential")
            .or_else(|| object.get("service"))
            .or_else(|| object.get("ref"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .unwrap_or_default()
    } else {
        ""
    };
    let service = raw.strip_prefix("credential://").unwrap_or(raw).trim();
    (!service.is_empty()).then(|| service.to_string())
}

fn provider_credential_binding_optional(value: &serde_json::Value, service: &str) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("optional"))
        .and_then(|item| item.as_bool())
        .unwrap_or_else(|| crate::config_manifest::is_mcp_optional_credential_service(service))
}

fn provider_credential_bindings(
    credential_refs_json: &str,
) -> AppResult<Vec<(String, serde_json::Value)>> {
    let value: serde_json::Value = serde_json::from_str(credential_refs_json)
        .map_err(|err| AppError::msg(format!("invalid credential refs JSON: {err}")))?;
    let mut bindings = Vec::new();
    if let Some(headers) = value.get("headers").and_then(|item| item.as_object()) {
        bindings.extend(
            headers
                .iter()
                .map(|(name, binding)| (format!("请求头 {name}"), binding.clone())),
        );
    }
    if let Some(env) = value.get("env").and_then(|item| item.as_object()) {
        bindings.extend(
            env.iter()
                .map(|(name, binding)| (format!("环境变量 {name}"), binding.clone())),
        );
    }
    Ok(bindings)
}

fn provider_credential_diagnostic_checks(
    _db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> AppResult<(Vec<WebEvidenceProviderDiagnosticCheck>, bool)> {
    let bindings = provider_credential_bindings(&provider.credential_refs_json)?;
    if bindings.is_empty() {
        return Ok((
            vec![provider_diagnostic_check("credential", true, "不需要凭据")],
            true,
        ));
    }

    let mut checks = Vec::new();
    let mut all_required_credentials_available = true;
    for (label, binding) in bindings {
        let Some(service) = provider_credential_service_from_binding(&binding) else {
            checks.push(provider_diagnostic_check(
                "credential",
                false,
                &format!("{label} 缺少凭据引用"),
            ));
            all_required_credentials_available = false;
            continue;
        };
        let optional = provider_credential_binding_optional(&binding, &service);
        let configured = crate::credentials::credential_available(&service)?;
        match configured {
            true => {
                checks.push(provider_diagnostic_check(
                    "credential",
                    true,
                    &format!("{label} Key 已绑定，请求将携带鉴权：{service}"),
                ));
            }
            false if optional => {
                checks.push(provider_diagnostic_check(
                    "credential",
                    true,
                    &format!("{label} 未配置 Key，将使用匿名额度：{service}"),
                ));
            }
            false => {
                checks.push(provider_diagnostic_check(
                    "credential",
                    false,
                    &format!("{label} 必填凭据缺失：{service}"),
                ));
                all_required_credentials_available = false;
            }
        }
    }
    Ok((checks, all_required_credentials_available))
}

fn provider_mapping_tool_name(mapping_json: Option<&str>) -> Option<String> {
    let value = mapping_json?.trim();
    if value.is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("tool")
                .or_else(|| parsed.get("tool_name"))
                .and_then(|tool| tool.as_str())
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(str::to_string)
        })
        .or_else(|| Some(value.to_string()))
}

fn diagnostic_error_message(error: &AppError) -> String {
    let redacted = crate::ai_runtime::trace::redact_classified_leaks(&error.to_string());
    const MAX_LEN: usize = 240;
    if redacted.chars().count() > MAX_LEN {
        format!("{}...", redacted.chars().take(MAX_LEN).collect::<String>())
    } else {
        redacted
    }
}

fn provider_transport_url(
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&provider.transport_config_json)
        .ok()
        .and_then(|value| {
            value
                .get("url")
                .and_then(|url| url.as_str())
                .map(str::to_string)
        })
}

fn provider_search_smoke_error_message(
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
    error: &AppError,
) -> String {
    let raw = error.to_string();
    let lower = raw.to_ascii_lowercase();
    let url = provider_transport_url(provider).unwrap_or_default();
    if lower.contains("auth_failed") && url.contains("mcp.tavily.com") {
        return "MCP 服务要求 OAuth 鉴权流程，当前预设不兼容".into();
    }
    diagnostic_error_message(error)
}

async fn run_mcp_search_smoke_test(
    db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> AppResult<crate::ai_runtime::web_evidence_broker::McpSearchProviderProbe> {
    crate::ai_runtime::web_evidence_broker::probe_mcp_search_provider_without_recording(
        db,
        &provider.id,
        "Iris note app",
        crate::ai_runtime::run_tool_loop::INITIAL_WEB_SEARCH_RESULTS,
        // A WebRequired Run gives its first MCP search attempt this same
        // bounded budget. Diagnostics must not advertise readiness using a
        // looser timeout than the actual preflight path.
        Duration::from_secs(15),
    )
    .await
}

fn mcp_search_probe_failure_message(
    failure: crate::ai_runtime::web_evidence_broker::McpApplicationFailureKind,
) -> &'static str {
    use crate::ai_runtime::web_evidence_broker::McpApplicationFailureKind;

    match failure {
        McpApplicationFailureKind::AuthFailed => {
            "Key 无效、已撤销、仍是旧 Key，或保存值错误包含 Bearer 前缀"
        }
        McpApplicationFailureKind::RateLimited => "搜索服务触发限流，请稍后重试",
        McpApplicationFailureKind::QuotaExceeded => "搜索服务额度已耗尽，请检查服务账户",
        McpApplicationFailureKind::InvalidArguments => "搜索工具参数无效，请检查工具映射",
        McpApplicationFailureKind::ProviderFailed => "搜索服务返回应用层错误",
    }
}

async fn provider_diagnostics_for_summary(
    db: &crate::storage::db::Database,
    provider: &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderSummary,
) -> AppResult<WebEvidenceProviderDiagnostics> {
    let transport_ok = provider.transport_kind == "https" || provider.transport_kind == "stdio";
    let mut checks = vec![
        provider_diagnostic_check("configured", true, "提供方记录存在"),
        provider_diagnostic_check(
            "enabled",
            provider.enabled,
            if provider.enabled {
                "提供方已启用"
            } else {
                "提供方未启用"
            },
        ),
        provider_diagnostic_check(
            "transport",
            transport_ok,
            if transport_ok {
                "连接方式支持 MCP 联网证据"
            } else {
                "连接方式不支持 MCP 联网证据"
            },
        ),
        provider_diagnostic_check(
            "searchMapping",
            provider.has_search_mapping,
            if provider.has_search_mapping {
                "已配置搜索映射"
            } else {
                "未配置搜索映射"
            },
        ),
        provider_diagnostic_check(
            "fetchMapping",
            provider.has_fetch_mapping,
            if provider.has_fetch_mapping {
                "已配置网页读取映射"
            } else {
                "未配置网页读取映射"
            },
        ),
    ];
    if provider.kind != "mcp" {
        checks.push(provider_diagnostic_check(
            "providerKind",
            false,
            "只有 MCP 提供方可作为可编辑联网证据提供方",
        ));
    }

    let (credential_checks, credentials_ok) = provider_credential_diagnostic_checks(db, provider)?;
    checks.extend(credential_checks);

    let mut can_use_for_search = provider.enabled && provider.has_search_mapping && credentials_ok;
    let mut can_use_for_fetch = provider.enabled && provider.has_fetch_mapping && credentials_ok;
    let circuit = crate::ai_runtime::circuit_breaker::inspect_readiness(&provider.id);
    let circuit_ready = circuit.request_allowed;
    checks.push(provider_diagnostic_check(
        "circuit",
        circuit_ready,
        match (circuit.status, circuit_ready) {
            (crate::ai_runtime::circuit_breaker::CircuitStatus::Open, false) => {
                "Agent 请求当前受熔断保护；请等待冷却结束后重试"
            }
            (crate::ai_runtime::circuit_breaker::CircuitStatus::Open, true) => {
                "熔断冷却已结束，下一次 Agent 请求将作为恢复探测"
            }
            (crate::ai_runtime::circuit_breaker::CircuitStatus::HalfOpen, _) => {
                "熔断器正在恢复探测阶段"
            }
            (crate::ai_runtime::circuit_breaker::CircuitStatus::Closed, _) => {
                "Agent 请求未受熔断限制"
            }
        },
    ));
    can_use_for_search = can_use_for_search && circuit_ready;
    can_use_for_fetch = can_use_for_fetch && circuit_ready;

    if provider.kind == "mcp" && provider.enabled {
        let options = crate::ai_runtime::mcp_host_runtime::McpHostRuntimeOptions {
            request_timeout: Duration::from_secs(20),
            max_stdout_line_bytes: 64 * 1024,
            max_stderr_bytes: 8 * 1024,
            cwd: None,
            stdio_session_pool: true,
            stdio_session_idle_timeout:
                crate::ai_runtime::mcp_host_runtime::DEFAULT_STDIO_SESSION_IDLE_TIMEOUT,
        };
        match crate::ai_runtime::mcp_host_runtime::discover_provider_tools_without_recording(
            db,
            &provider.id,
            options,
        )
        .await
        {
            Ok(discovery) => {
                let tool_names = discovery
                    .tools
                    .iter()
                    .map(|tool| tool.name.as_str())
                    .collect::<HashSet<_>>();
                checks.push(provider_diagnostic_check(
                    "liveConnection",
                    true,
                    "MCP 服务已响应 tools/list",
                ));
                if let Some(tool) =
                    provider_mapping_tool_name(provider.web_search_mapping_json.as_deref())
                {
                    let exists = tool_names.contains(tool.as_str());
                    can_use_for_search = can_use_for_search && exists;
                    checks.push(provider_diagnostic_check(
                        "searchToolLive",
                        exists,
                        &if exists {
                            format!("已找到搜索工具 '{tool}'")
                        } else {
                            format!("MCP 服务未报告搜索工具 '{tool}'")
                        },
                    ));
                    if exists && provider.web_search_mapping_json.is_some() {
                        match run_mcp_search_smoke_test(db, provider).await {
                            Ok(probe) => {
                                let parseable = probe.diagnostic.usable_https_row_count > 0;
                                let call_succeeded = probe.diagnostic.application_failure.is_none();
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeAuthHeader",
                                    probe.auth_header_present,
                                    if probe.auth_header_present {
                                        "搜索探针请求将携带 Authorization"
                                    } else {
                                        "搜索探针未携带 Authorization（匿名额度）"
                                    },
                                ));
                                if let Ok(fingerprint) = crate::ai_runtime::mcp_host_runtime::provider_http_auth_fingerprint(
                                    db,
                                    &provider.id,
                                ) {
                                    checks.push(provider_diagnostic_check(
                                        "authFingerprint",
                                        true,
                                        &fingerprint.summary(),
                                    ));
                                }
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeLive",
                                    call_succeeded,
                                    &if let Some(failure) = probe.diagnostic.application_failure {
                                        format!(
                                            "搜索调用失败：{}",
                                            mcp_search_probe_failure_message(failure)
                                        )
                                    } else {
                                        format!(
                                            "本次探针调用正常，解析出 {} 条记录，其中 {} 条可安全注册为 HTTPS 证据；{}",
                                            probe.diagnostic.parsed_row_count,
                                            probe.diagnostic.usable_https_row_count,
                                            probe.summary()
                                        )
                                    },
                                ));
                                can_use_for_search =
                                    can_use_for_search && call_succeeded && parseable;
                                checks.push(provider_diagnostic_check(
                                    "searchResultParseLive",
                                    parseable,
                                    if parseable {
                                        "本次探针结果已归一化为可注册的联网证据"
                                    } else {
                                        "本次探针未返回可安全注册的 HTTPS 证据"
                                    },
                                ));
                            }
                            Err(error) => {
                                can_use_for_search = false;
                                checks.push(provider_diagnostic_check(
                                    "searchSmokeLive",
                                    false,
                                    &format!(
                                        "MCP 搜索 smoke test 失败：{}",
                                        provider_search_smoke_error_message(provider, &error)
                                    ),
                                ));
                            }
                        }
                    }
                }
                if let Some(tool) =
                    provider_mapping_tool_name(provider.web_fetch_mapping_json.as_deref())
                {
                    let exists = tool_names.contains(tool.as_str());
                    can_use_for_fetch = can_use_for_fetch && exists;
                    checks.push(provider_diagnostic_check(
                        "fetchToolLive",
                        exists,
                        &if exists {
                            format!("已找到网页读取工具 '{tool}'")
                        } else {
                            format!("MCP 服务未报告网页读取工具 '{tool}'")
                        },
                    ));
                }
            }
            Err(error) => {
                can_use_for_search = false;
                can_use_for_fetch = false;
                checks.push(provider_diagnostic_check(
                    "liveConnection",
                    false,
                    &format!(
                        "MCP 实时探测失败：{}",
                        provider_search_smoke_error_message(provider, &error)
                    ),
                ));
            }
        }
    }

    let failures = checks
        .iter()
        .filter(|check| check.status != "pass")
        .map(|check| check.message.clone())
        .collect::<Vec<_>>();
    let mapping_status =
        provider_mapping_status(provider.has_search_mapping, provider.has_fetch_mapping);
    let is_runtime_selected =
        crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(db)
            .ok()
            .is_some_and(|selected| selected.id == provider.id);
    Ok(WebEvidenceProviderDiagnostics {
        provider_id: Some(provider.id.clone()),
        is_runtime_selected,
        status: if failures.is_empty() && provider.enabled {
            "ready".into()
        } else if provider.enabled && mapping_status == "complete" {
            "degraded".into()
        } else {
            provider_diagnostic_status(provider.enabled, &mapping_status)
        },
        failures,
        checks,
        can_use_for_search,
        can_use_for_fetch,
    })
}

fn mapping_json_from_tool_name(value: Option<String>) -> AppResult<Option<String>> {
    let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if value.starts_with('{') || value.starts_with('[') {
        serde_json::from_str::<serde_json::Value>(&value)
            .map_err(|err| AppError::msg(format!("invalid provider mapping JSON: {err}")))?;
        return Ok(Some(value));
    }
    Ok(Some(
        serde_json::json!({
            "tool": value,
        })
        .to_string(),
    ))
}

fn provider_input_to_registry(
    input: WebEvidenceProviderInput,
) -> AppResult<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput> {
    let provider_kind = input.provider_kind.trim().to_lowercase();
    let transport_kind = input
        .transport_kind
        .unwrap_or_else(|| {
            if provider_kind == "native" {
                "native".into()
            } else {
                "stdio".into()
            }
        })
        .trim()
        .to_lowercase();
    Ok(
        crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: input.id,
            name: input.name,
            kind: provider_kind,
            enabled: input.enabled,
            transport_kind,
            transport_config_json: input.transport_config_json,
            credential_refs_json: input.credential_refs_json,
            web_search_mapping_json: mapping_json_from_tool_name(input.search_mapping)?,
            web_fetch_mapping_json: mapping_json_from_tool_name(input.fetch_mapping)?,
        },
    )
}

#[tauri::command]
pub async fn skills_create_draft(
    state: State<'_, Arc<AppState>>,
    request: SkillCreateDraftRequest,
) -> AppResult<SkillDraftDto> {
    let description = request
        .description
        .as_deref()
        .unwrap_or("Iris prompt-only skill");
    let body = request
        .body
        .as_deref()
        .unwrap_or("Use this skill for the confirmed scope.");
    let scope_yaml = request
        .scope_rules
        .iter()
        .map(|rule| format!("  - kind: {}\n    pattern: {:?}", rule.kind, rule.pattern))
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = format!(
        "---\nname: {}\ndescription: {}\nscope:\n{}\n---\n\n{}\n",
        request.name, description, scope_yaml, body
    );
    let digest = Sha256::digest(markdown.as_bytes());
    let vault = state.vault_path()?;
    let slug = skill_draft_slug(&request.name);
    Ok(SkillDraftDto {
        target_path: vault
            .join(".iris")
            .join("skills")
            .join(slug)
            .join("SKILL.md")
            .to_string_lossy()
            .to_string(),
        name: request.name,
        markdown,
        scope_rules: request.scope_rules,
        content_hash: hex::encode(digest),
    })
}

#[tauri::command]
pub async fn skills_confirm(
    state: State<'_, Arc<AppState>>,
    draft: SkillDraftDto,
) -> AppResult<()> {
    let digest = Sha256::digest(draft.markdown.as_bytes());
    let actual_hash = hex::encode(digest);
    if actual_hash != draft.content_hash {
        return Err(AppError::msg(
            "Skill draft hash does not match confirmed content",
        ));
    }
    let vault = state.vault_path()?;
    crate::ai_runtime::skills::write_confirmed_skill_content(
        &vault,
        Path::new(&draft.target_path),
        crate::ai_runtime::skills::SkillScope::Vault,
        &draft.markdown,
    )?;
    Ok(())
}

fn skill_draft_slug(name: &str) -> String {
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "skill".into()
    } else {
        slug
    }
}
#[tauri::command]
pub async fn prompt_profile_get(
    state: State<'_, Arc<AppState>>,
) -> AppResult<crate::ai_runtime::prompt_profile::PromptProfile> {
    crate::ai_runtime::prompt_profile::PromptProfile::load(&state.db)
}

#[tauri::command]
pub async fn prompt_profile_set(
    state: State<'_, Arc<AppState>>,
    profile: crate::ai_runtime::prompt_profile::PromptProfile,
) -> AppResult<()> {
    crate::ai_runtime::prompt_profile::PromptProfile::save(&state.db, &profile)?;
    crate::llm::safe_lock(&state.ai.context_cache).clear();
    Ok(())
}

/// List built-in prompt profile presets.
#[tauri::command]
pub fn prompt_profile_presets() -> Vec<serde_json::Value> {
    crate::ai_runtime::prompt_profile::preset_templates()
        .into_iter()
        .map(|(label, profile)| serde_json::json!({ "label": label, "profile": profile }))
        .collect()
}
