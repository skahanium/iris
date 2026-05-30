//! User-configurable AI persona / writing style for environment injection.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::db::Database;

const PROFILE_KEY: &str = "ai_prompt_profile";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptProfile {
    pub persona: String,
    pub writing_style: String,
    #[serde(default)]
    pub custom_rules: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

/// Built-in prompt profile presets for quick selection.
pub fn preset_templates() -> Vec<(&'static str, PromptProfile)> {
    vec![
        (
            "学术严谨",
            PromptProfile {
                persona: "严谨、客观的学术助手，重视证据与引用。".into(),
                writing_style: "结构清晰、术语准确、避免口语化。".into(),
                custom_rules: vec![
                    "优先引用上下文证据。".into(),
                    "不确定时明确说明局限。".into(),
                ],
                language: "zh-CN".into(),
            },
        ),
        (
            "创意写作",
            PromptProfile {
                persona: "富有想象力的写作伙伴，善于拓展情节与人物。".into(),
                writing_style: "生动、有画面感，适度修辞。".into(),
                custom_rules: vec!["保持与既有设定一致。".into()],
                language: "zh-CN".into(),
            },
        ),
        (
            "简洁高效",
            PromptProfile {
                persona: "高效执行型助手，直达要点。".into(),
                writing_style: "短句、列表、少废话。".into(),
                custom_rules: vec!["默认不超过三段。".into()],
                language: "zh-CN".into(),
            },
        ),
    ]
}

impl PromptProfile {
    pub fn load(db: &Database) -> AppResult<Self> {
        db.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT value FROM user_profile WHERE key = ?1",
                [PROFILE_KEY],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Self::default()),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn save(db: &Database, profile: &Self) -> AppResult<()> {
        let json = serde_json::to_string(profile)?;
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO user_profile (key, value, source, confidence, is_active, updated_at)
                 VALUES (?1, ?2, 'user', 1.0, 1, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                rusqlite::params![PROFILE_KEY, json, now],
            )?;
            Ok(())
        })
    }

    pub fn to_system_prompt_fragment(&self) -> String {
        if self.persona.is_empty() && self.writing_style.is_empty() && self.custom_rules.is_empty()
        {
            return String::new();
        }
        let mut s = String::from("## 用户 AI 人格配置\n\n");
        if !self.persona.is_empty() {
            s.push_str(&format!("**人格**：{}\n\n", self.persona));
        }
        if !self.writing_style.is_empty() {
            s.push_str(&format!("**写作风格**：{}\n\n", self.writing_style));
        }
        if !self.language.is_empty() {
            s.push_str(&format!("**回答语言**：{}\n\n", self.language));
        }
        if !self.custom_rules.is_empty() {
            s.push_str("**自定义规则**：\n");
            for rule in &self.custom_rules {
                s.push_str(&format!("- {rule}\n"));
            }
        }
        s
    }
}
