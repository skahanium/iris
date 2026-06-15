//! Environment map builder — project awareness for the harness system prompt.

use std::path::Path;

use crate::ai_runtime::model_gateway::ModelGateway;
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::runtime_context::{build_runtime_context_prompt, RuntimeContextInput};
use crate::ai_runtime::{AiScene, ToolSpec};
use crate::error::AppResult;
use crate::storage::db::Database;

/// Inputs for building the environment map.
#[derive(Debug, Clone)]
pub struct EnvironmentInput<'a> {
    pub scene: AiScene,
    pub note_path: Option<&'a str>,
    pub note_title: Option<&'a str>,
    pub selection_excerpt: Option<&'a str>,
    pub tools: &'a [ToolSpec],
    pub web_search_enabled: bool,
    pub attachment_count: usize,
}

/// Build layered environment text for system prompt injection.
pub fn build_environment_map(
    db: &Database,
    vault: &Path,
    input: &EnvironmentInput<'_>,
) -> AppResult<String> {
    let mut sections = Vec::new();

    sections.push(build_runtime_context_prompt(
        db,
        vault,
        &RuntimeContextInput {
            web_search_enabled: input.web_search_enabled,
            note_path: input.note_path,
            note_title: input.note_title,
            selection_excerpt: input.selection_excerpt,
            attachment_count: input.attachment_count,
            tools: input.tools,
        },
    ));
    sections.push(capabilities_section(input.tools));
    sections.push(scene_focus_section(input.scene));

    if let Some(doc) = current_document_section(input) {
        sections.push(doc);
    }

    if let Some(path) = input.note_path {
        if let Ok(backlinks) = backlinks_section(db, path) {
            if !backlinks.is_empty() {
                sections.push(backlinks);
            }
        }
    }

    if let Ok(vault_outline) = vault_structure_section(db, vault) {
        if !vault_outline.is_empty() {
            sections.push(vault_outline);
        }
    }

    if let Ok(rules) = ModelGateway::load_active_rules_for_scene(db, input.scene) {
        if !rules.is_empty() {
            let mut block = String::from("## 用户规则\n\n");
            for rule in rules {
                block.push_str(&format!("- {rule}\n"));
            }
            sections.push(block);
        }
    }

    if let Ok(profile) = PromptProfile::load(db) {
        let fragment = profile.to_system_prompt_fragment();
        if !fragment.is_empty() {
            sections.push(fragment);
        }
    }

    Ok(sections.join("\n\n"))
}

fn capabilities_section(tools: &[ToolSpec]) -> String {
    let mut s = String::from(
        "## 环境：Iris 与你的能力\n\n\
         Iris 是本地 Markdown 笔记本应用；`.md` 文件是数据权威来源，SQLite 仅作索引缓存。\n\
         你应通过工具主动获取信息，而非假设未检索到的内容存在。\n\n\
         ### 可用工具\n",
    );
    for tool in tools {
        s.push_str(&format!(
            "- **{}**：{}（{}）\n",
            tool.name,
            tool.description,
            if tool.requires_confirmation {
                "写入需用户确认"
            } else {
                "只读，可自动执行"
            }
        ));
    }
    s
}

fn scene_focus_section(scene: AiScene) -> String {
    let focus = match scene {
        AiScene::KnowledgeLookup => "知识查阅：检索、解释、引用本地材料",
        AiScene::ExemplarLearning => "范文学习：结构、句式与可复用模板",
        AiScene::DraftingAssist => "文稿创作：低干扰写作辅助与补丁建议",
        AiScene::ResearchSynthesis => "研究综合：多材料论证与证据缺口",
    };
    format!("## 当前任务侧重\n\n{focus}\n")
}

fn current_document_section(input: &EnvironmentInput<'_>) -> Option<String> {
    let path = input.note_path?;
    let title = input.note_title.filter(|t| !t.is_empty()).unwrap_or(path);
    let mut block = format!("## 当前文档\n\n- 路径: `{path}`\n- 标题: {title}\n");
    if let Some(sel) = input.selection_excerpt.filter(|s| !s.is_empty()) {
        let excerpt: String = sel.chars().take(400).collect();
        let suffix = if sel.chars().count() > 400 { "…" } else { "" };
        block.push_str(&format!("\n### 用户选区\n\n{excerpt}{suffix}\n"));
    }
    block.push_str("\n全文可通过 `read_note` 工具读取。\n");
    Some(block)
}

fn backlinks_section(db: &Database, path: &str) -> AppResult<String> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT f.path, f.title
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
             ORDER BY f.title
             LIMIT 12",
        )?;
        let rows = stmt.query_map([path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut block = String::from("## 反向链接（摘要）\n\n");
        let mut count = 0;
        for row in rows {
            let (p, title) = row?;
            block.push_str(&format!("- [{title}]({p})\n"));
            count += 1;
        }
        if count == 0 {
            return Ok(String::new());
        }
        block.push_str("\n更多链接可用 `get_backlinks` 工具查询。\n");
        Ok(block)
    })
}

fn vault_structure_section(db: &Database, _vault: &Path) -> AppResult<String> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
             ORDER BY path
             LIMIT 80",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut by_top: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut lines = Vec::new();
        for row in rows {
            let (path, title) = row?;
            let top = path.split('/').next().unwrap_or("").to_string();
            *by_top.entry(top).or_insert(0) += 1;
            if lines.len() < 25 {
                lines.push(format!("- `{path}` — {title}"));
            }
        }
        let mut block = String::from("## 知识库结构（摘要）\n\n");
        block.push_str("顶层目录分布：\n");
        for (dir, count) in by_top {
            let label = if dir.is_empty() { "(根目录)" } else { &dir };
            block.push_str(&format!("- {label}: {count} 篇\n"));
        }
        if !lines.is_empty() {
            block.push_str("\n部分笔记：\n");
            for line in lines {
                block.push_str(&format!("{line}\n"));
            }
        }
        block.push_str("\n完整列表请用 `list_vault` 工具。\n");
        Ok(block)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::ToolAccessLevel;
    use crate::storage::db::Database;

    #[test]
    fn capabilities_lists_tool_names() {
        let tools = vec![ToolSpec {
            name: "search_hybrid".into(),
            description: "混合搜索".into(),
            input_schema: serde_json::json!({}),
            access_level: ToolAccessLevel::ReadIndex,
            requires_confirmation: false,
            max_results: None,
            scene_affinity: vec![],
        }];
        let text = capabilities_section(&tools);
        assert!(text.contains("search_hybrid"));
        assert!(text.contains("Iris"));
    }

    #[test]
    fn environment_includes_runtime_context() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let db = Database::open(&dir.path().join("iris.db")).unwrap();
        let tools = vec![
            ToolSpec {
                name: "system_time_now".into(),
                description: "读取当前本机时间".into(),
                input_schema: serde_json::json!({}),
                access_level: ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                scene_affinity: vec![],
            },
            ToolSpec {
                name: "web_search".into(),
                description: "联网搜索".into(),
                input_schema: serde_json::json!({}),
                access_level: ToolAccessLevel::Network,
                requires_confirmation: false,
                max_results: Some(8),
                scene_affinity: vec![AiScene::KnowledgeLookup],
            },
        ];

        let text = build_environment_map(
            &db,
            &vault,
            &EnvironmentInput {
                scene: AiScene::KnowledgeLookup,
                note_path: Some("today.md"),
                note_title: Some("Today"),
                selection_excerpt: Some("selected text"),
                tools: &tools,
                web_search_enabled: true,
                attachment_count: 1,
            },
        )
        .unwrap();

        assert!(text.contains("## 当前运行环境"));
        assert!(text.contains("本机日期"));
        assert!(text.contains("星期"));
        assert!(text.contains("时区"));
        assert!(text.contains("联网工具: 已启用"));
        assert!(text.contains("附件: 1 个"));
    }
}
