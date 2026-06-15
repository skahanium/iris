//! Runtime context exposed to the assistant as trusted local facts.

use chrono::{Datelike, Local, Timelike};
use serde::Serialize;
use std::path::Path;

use crate::ai_runtime::{CapabilitySlot, ToolSpec};
use crate::app::AppState;
use crate::credentials;
use crate::error::AppResult;
use crate::llm::providers::credential_service;
use crate::storage::db::Database;

#[derive(Debug, Clone, Serialize)]
/// Trusted local date/time facts for the current assistant run.
pub struct RuntimeTimeContext {
    pub kind: &'static str,
    pub local_date: String,
    pub local_time: String,
    pub local_datetime: String,
    pub weekday_zh: String,
    pub weekday_en: String,
    pub utc_offset: String,
    pub timezone: String,
}

#[derive(Debug, Clone)]
/// Inputs used to render runtime facts into the system prompt.
pub struct RuntimeContextInput<'a> {
    pub web_search_enabled: bool,
    pub note_path: Option<&'a str>,
    pub note_title: Option<&'a str>,
    pub selection_excerpt: Option<&'a str>,
    pub attachment_count: usize,
    pub tools: &'a [ToolSpec],
}

#[derive(Debug, Clone, Serialize)]
/// Snapshot of the current Iris UI/application context.
pub struct AppContextSnapshot {
    pub kind: &'static str,
    pub vault_path: Option<String>,
    pub note_path: Option<String>,
    pub note_title: Option<String>,
    pub file_id: Option<i64>,
    pub selection_present: bool,
    pub selection_excerpt: Option<String>,
    pub attachment_count: usize,
}

#[derive(Debug, Clone, Serialize)]
/// One tool exposed in the assistant capability snapshot.
pub struct ToolCapabilitySnapshot {
    pub name: String,
    pub requires_confirmation: bool,
    pub access_level: String,
}

#[derive(Debug, Clone, Serialize)]
/// Safe model-slot summary that never exposes API key material.
pub struct ModelSlotSnapshot {
    pub slot: String,
    pub provider_id: String,
    pub model: String,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize)]
/// Current AI capability summary, including tools and configured model slots.
pub struct CapabilitySnapshot {
    pub kind: &'static str,
    pub web_search_enabled: bool,
    pub vision: ModelSlotSnapshot,
    pub slots: Vec<ModelSlotSnapshot>,
    pub tools: Vec<ToolCapabilitySnapshot>,
}

/// Return the current local date/time as structured trusted facts.
pub fn current_time_context() -> RuntimeTimeContext {
    let now = Local::now();
    let weekday = now.weekday();
    RuntimeTimeContext {
        kind: "system_time",
        local_date: format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day()),
        local_time: format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second()),
        local_datetime: now.to_rfc3339(),
        weekday_zh: format!("星期{}", weekday_zh_suffix(weekday)),
        weekday_en: format!("{weekday:?}"),
        utc_offset: now.offset().to_string(),
        timezone: std::env::var("TZ").unwrap_or_else(|_| now.offset().to_string()),
    }
}

/// Render a short Chinese line for web-search context that needs today's date.
pub fn local_date_line_zh() -> String {
    let now = current_time_context();
    format!(
        "【本机日期】{}（{}，{}）。回答「今天几号/星期几/当前日期」类问题时以本机日期为准，网页摘要仅作补充。",
        now.local_date, now.weekday_zh, now.timezone
    )
}

/// Render the runtime context block injected into assistant system prompts.
pub fn build_runtime_context_prompt(
    db: &Database,
    vault: &Path,
    input: &RuntimeContextInput<'_>,
) -> String {
    let time = current_time_context();
    let capabilities = capability_snapshot(db, input.web_search_enabled, input.tools);
    let vault_name = vault
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("(未命名 vault)");
    let selection = input
        .selection_excerpt
        .filter(|excerpt| !excerpt.trim().is_empty())
        .map(|excerpt| excerpt.chars().take(120).collect::<String>());
    let configured_slots = capabilities
        .slots
        .iter()
        .map(|slot| {
            format!(
                "{}={}@{}({})",
                slot.slot,
                slot.model,
                slot.provider_id,
                if slot.configured {
                    "已配置"
                } else {
                    "未配置 Key"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let default_tools = input
        .tools
        .iter()
        .filter(|tool| !tool.requires_confirmation)
        .map(|tool| tool.name.as_str())
        .take(16)
        .collect::<Vec<_>>()
        .join(", ");

    let mut block = format!(
        "## 当前运行环境\n\n\
         - 本机日期: {}（{}）\n\
         - 本机时间: {} {}\n\
         - 时区: {}\n\
         - Locale: {}\n\
         - 操作系统: {}\n\
         - Iris 版本: {}\n\
         - 当前 vault: {} (`{}`)\n\
         - 联网工具: {}\n\
         - Vision 模型: {}\n\
         - 模型槽位: {}\n\
         - 可自动调用的只读工具: {}\n\
         - 附件: {} 个\n",
        time.local_date,
        time.weekday_zh,
        time.local_time,
        time.utc_offset,
        time.timezone,
        std::env::var("LANG").unwrap_or_else(|_| "unknown".into()),
        std::env::consts::OS,
        env!("CARGO_PKG_VERSION"),
        vault_name,
        vault.display(),
        if input.web_search_enabled {
            "已启用"
        } else {
            "未启用"
        },
        if capabilities.vision.configured {
            format!(
                "已配置 {}@{}",
                capabilities.vision.model, capabilities.vision.provider_id
            )
        } else {
            "未配置".into()
        },
        if configured_slots.is_empty() {
            "(无)".into()
        } else {
            configured_slots
        },
        if default_tools.is_empty() {
            "(无)".into()
        } else {
            default_tools
        },
        input.attachment_count
    );
    if let Some(path) = input.note_path {
        block.push_str(&format!("- 当前笔记路径: `{path}`\n"));
    }
    if let Some(title) = input.note_title.filter(|title| !title.trim().is_empty()) {
        block.push_str(&format!("- 当前笔记标题: {title}\n"));
    }
    if let Some(excerpt) = selection {
        block.push_str(&format!("- 当前选区摘要: {excerpt}\n"));
    }
    block.push_str(
        "\n本区块是可信的本机运行时事实。回答当前日期、时间、星期、应用能力、当前笔记或附件状态时优先使用它；只有外部世界事实才需要联网搜索。\n",
    );
    block
}

/// Build a safe snapshot of the current app context for the assistant.
pub fn app_context_snapshot(
    state: &AppState,
    note_path: Option<&str>,
    file_id: Option<i64>,
    attachment_count: usize,
) -> AppContextSnapshot {
    AppContextSnapshot {
        kind: "app_context",
        vault_path: state
            .vault_path()
            .ok()
            .map(|path| path.to_string_lossy().to_string()),
        note_path: note_path.map(str::to_string),
        note_title: note_path.and_then(|path| note_title(&state.db, path).ok().flatten()),
        file_id,
        selection_present: false,
        selection_excerpt: None,
        attachment_count,
    }
}

/// Build a safe capability snapshot without reading any credential plaintext.
pub fn capability_snapshot(
    db: &Database,
    web_search_enabled: bool,
    tools: &[ToolSpec],
) -> CapabilitySnapshot {
    let slots = model_slot_snapshots(db);
    let vision = slots
        .iter()
        .find(|slot| slot.slot == "vision")
        .cloned()
        .unwrap_or_else(|| ModelSlotSnapshot {
            slot: "vision".into(),
            provider_id: String::new(),
            model: String::new(),
            configured: false,
        });
    CapabilitySnapshot {
        kind: "capabilities",
        web_search_enabled,
        vision,
        slots,
        tools: tools
            .iter()
            .map(|tool| ToolCapabilitySnapshot {
                name: tool.name.clone(),
                requires_confirmation: tool.requires_confirmation,
                access_level: format!("{:?}", tool.access_level),
            })
            .collect(),
    }
}

fn model_slot_snapshots(db: &Database) -> Vec<ModelSlotSnapshot> {
    let Ok(config) = crate::llm::config::load(db) else {
        return Vec::new();
    };
    config
        .slots
        .iter()
        .map(|(slot, route)| {
            let service = credential_service(&route.provider_id);
            let configured = credentials::api_key_configured(db, &service).unwrap_or(false)
                || !crate::llm::providers::requires_api_key(&route.provider_id);
            ModelSlotSnapshot {
                slot: slot.clone(),
                provider_id: route.provider_id.clone(),
                model: route.model.clone(),
                configured,
            }
        })
        .collect()
}

fn note_title(db: &Database, path: &str) -> AppResult<Option<String>> {
    db.with_conn(|conn| {
        Ok(conn
            .query_row("SELECT title FROM files WHERE path = ?1", [path], |row| {
                row.get::<_, String>(0)
            })
            .ok())
    })
}

fn weekday_zh_suffix(weekday: chrono::Weekday) -> &'static str {
    match weekday {
        chrono::Weekday::Mon => "一",
        chrono::Weekday::Tue => "二",
        chrono::Weekday::Wed => "三",
        chrono::Weekday::Thu => "四",
        chrono::Weekday::Fri => "五",
        chrono::Weekday::Sat => "六",
        chrono::Weekday::Sun => "日",
    }
}

/// Convert all exposable catalog entries to `ToolSpec` values.
pub fn all_catalog_tools_as_specs() -> Vec<ToolSpec> {
    crate::ai_runtime::tool_catalog::TOOL_CATALOG
        .iter()
        .filter(|entry| {
            entry.implementation
                != crate::ai_runtime::tool_catalog::ToolImplementationStatus::Planned
        })
        .map(|entry| ToolSpec {
            name: entry.name.into(),
            description: entry.description.into(),
            input_schema: entry.input_schema.clone(),
            access_level: entry.access_level,
            requires_confirmation: entry.requires_confirmation,
            max_results: entry.max_results,
            scene_affinity: entry.scene_affinity.to_vec(),
        })
        .collect()
}

#[allow(dead_code)]
fn _slot_wire(slot: CapabilitySlot) -> &'static str {
    match slot {
        CapabilitySlot::Fast => "fast",
        CapabilitySlot::Writer => "writer",
        CapabilitySlot::Reasoner => "reasoner",
        CapabilitySlot::LongContext => "long_context",
        CapabilitySlot::Vision => "vision",
        CapabilitySlot::AgentTools => "agent_tools",
        CapabilitySlot::Embedding => "embedding",
        CapabilitySlot::Reranker => "reranker",
        CapabilitySlot::LocalPrivate => "local_private",
    }
}
