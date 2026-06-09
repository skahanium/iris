//! Environment map builder 鈥?project awareness for the harness system prompt.

use std::path::Path;

use crate::ai_runtime::model_gateway::ModelGateway;
use crate::ai_runtime::prompt_profile::PromptProfile;
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
}

/// Build layered environment text for system prompt injection.
pub fn build_environment_map(
    db: &Database,
    vault: &Path,
    input: &EnvironmentInput<'_>,
) -> AppResult<String> {
    let mut sections = Vec::new();

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
            let mut block = String::from("## 鐢ㄦ埛瑙勫垯\n\n");
            for rule in rules {
                block.push_str(&format!("- {rule}\n"));
            }
            sections.push(block);
        }
    }

    if let Ok(profile) = PromptProfile::load(db) {
        if !fragment.is_empty() {
            sections.push(fragment);
        }
    }

    Ok(sections.join("\n\n"))
}

fn capabilities_section(tools: &[ToolSpec]) -> String {
    let mut s = String::from(
        "## 鐜锛欼ris 涓庝綘鐨勮兘鍔沑n\n\
         Iris 鏄湰鍦?Markdown 绗旇鏈簲鐢紱`.md` 鏂囦欢鏄暟鎹潈濞佹潵婧愶紝SQLite 浠呬綔绱㈠紩缂撳瓨銆俓n\
         浣犲簲閫氳繃宸ュ叿涓诲姩鑾峰彇淇℃伅锛岃€岄潪鍋囪鏈绱㈠埌鐨勫唴瀹瑰瓨鍦ㄣ€俓n\n\
         ### 鍙敤宸ュ叿\n",
    );
    for tool in tools {
        s.push_str(&format!(
            "- **{}**锛歿}锛坽}锛塡n",
            tool.name,
            tool.description,
            if tool.requires_confirmation {
                "鍐欏叆闇€鐢ㄦ埛纭"
            } else {
                "鍙锛屽彲鑷姩鎵ц"
            }
        ));
    }
    s
}

fn scene_focus_section(scene: AiScene) -> String {
    let focus = match scene {
        AiScene::KnowledgeLookup => "鐭ヨ瘑鏌ラ槄锛氭绱€佽В閲娿€佸紩鐢ㄦ湰鍦版潗鏂?,
        AiScene::ExemplarLearning => "鑼冩枃瀛︿範锛氱粨鏋勩€佸彞寮忎笌鍙鐢ㄦā鏉?,
        AiScene::DraftingAssist => "鏂囩鍒涗綔锛氫綆骞叉壈鍐欎綔杈呭姪涓庤ˉ涓佸缓璁?,
        AiScene::ResearchSynthesis => "鐮旂┒缁煎悎锛氬鏉愭枡璁鸿瘉涓庤瘉鎹己鍙?,
    };
    format!("## 褰撳墠浠诲姟渚ч噸\n\n{focus}\n")
}

fn current_document_section(input: &EnvironmentInput<'_>) -> Option<String> {
    let path = input.note_path?;
    let title = input.note_title.filter(|t| !t.is_empty()).unwrap_or(path);
    let mut block = format!("## 褰撳墠鏂囨。\n\n- 璺緞: `{path}`\n- 鏍囬: {title}\n");
    if let Some(sel) = input.selection_excerpt.filter(|s| !s.is_empty()) {
        let excerpt: String = sel.chars().take(400).collect();
        let suffix = if sel.chars().count() > 400 { "鈥? } else { "" };
        block.push_str(&format!("\n### 鐢ㄦ埛閫夊尯\n\n{excerpt}{suffix}\n"));
    }
    block.push_str("\n鍏ㄦ枃鍙€氳繃 `read_note` 宸ュ叿璇诲彇銆俓n");
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
        let mut block = String::from("## 鍙嶅悜閾炬帴锛堟憳瑕侊級\n\n");
        let mut count = 0;
        for row in rows {
            let (p, title) = row?;
            block.push_str(&format!("- [{title}]({p})\n"));
            count += 1;
        }
        if count == 0 {
            return Ok(String::new());
        }
        block.push_str("\n鏇村閾炬帴鍙敤 `get_backlinks` 宸ュ叿鏌ヨ銆俓n");
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
                lines.push(format!("- `{path}` 鈥?{title}"));
            }
        }
        let mut block = String::from("## 鐭ヨ瘑搴撶粨鏋勶紙鎽樿锛塡n\n");
        block.push_str("椤跺眰鐩綍鍒嗗竷锛歕n");
        for (dir, count) in by_top {
            let label = if dir.is_empty() { "(鏍圭洰褰?" } else { &dir };
            block.push_str(&format!("- {label}: {count} 绡嘰n"));
        }
        if !lines.is_empty() {
            block.push_str("\n閮ㄥ垎绗旇锛歕n");
            for line in lines {
                block.push_str(&format!("{line}\n"));
            }
        }
        block.push_str("\n瀹屾暣鍒楄〃璇风敤 `list_vault` 宸ュ叿銆俓n");
        Ok(block)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::ToolAccessLevel;

    #[test]
    fn capabilities_lists_tool_names() {
        let tools = vec![ToolSpec {
            name: "search_hybrid".into(),
            description: "娣峰悎鎼滅储".into(),
            input_schema: serde_json::json!({}),
            access_level: ToolAccessLevel::ReadIndex,
            scene_allowlist: vec![],
            requires_confirmation: false,
            max_results: None,
            scene_affinity: vec![],
        }];
        let text = capabilities_section(&tools);
        assert!(text.contains("search_hybrid"));
        assert!(text.contains("Iris"));
    }
}
