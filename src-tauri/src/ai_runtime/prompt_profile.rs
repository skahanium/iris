//! User-configurable AI persona / writing style for environment injection.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::storage::db::Database;

const PROFILE_KEY: &str = "ai_prompt_profile";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptProfile {
    #[serde(default = "default_display_name")]
    pub display_name: String,
    #[serde(default)]
    pub avatar_emoji: Option<String>,
    #[serde(default)]
    pub persona: String,
    #[serde(default)]
    pub writing_style: String,
    #[serde(default)]
    pub custom_rules: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_display_name() -> String {
    "砚".to_string()
}

fn default_language() -> String {
    "zh-CN".to_string()
}

impl Default for PromptProfile {
    fn default() -> Self {
        Self {
            display_name: default_display_name(),
            avatar_emoji: None,
            persona: String::new(),
            writing_style: String::new(),
            custom_rules: Vec::new(),
            language: default_language(),
        }
    }
}

/// Built-in prompt profile presets for quick selection.
pub fn preset_templates() -> Vec<(&'static str, PromptProfile)> {
    vec![
        (
            "学术严谨",
            PromptProfile {
                display_name: "砚".into(),
                avatar_emoji: Some("📚".into()),
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
                display_name: "砚".into(),
                avatar_emoji: Some("🖋️".into()),
                persona: "富有想象力的写作伙伴，善于拓展情节与人物。".into(),
                writing_style: "生动、有画面感，适度修辞。".into(),
                custom_rules: vec!["保持与既有设定一致。".into()],
                language: "zh-CN".into(),
            },
        ),
        (
            "简洁高效",
            PromptProfile {
                display_name: "砚".into(),
                avatar_emoji: Some("⚡".into()),
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
            s.push('\n');
        }
        s.push_str(PERSONA_REPLY_DISCIPLINE);
        s
    }
}

/// Standing reply discipline injected with any non-empty persona fragment.
const PERSONA_REPLY_DISCIPLINE: &str = "**回复纪律**：\n\
- 短问候或寒暄时：仅简短回应，并邀请用户说明具体任务。\n\
- 禁止主动复述人格、单位/角色、职责清单或能力介绍。\n\
- 仅当用户明确询问「你是谁」或「你能做什么」时，才用一两句话说明身份。\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_includes_display_name() {
        let profile = PromptProfile::default();
        assert_eq!(profile.display_name, "砚");
        assert!(profile.avatar_emoji.is_none());
    }

    #[test]
    fn deserializes_legacy_profile_without_display_fields() {
        let json = r#"{"persona":"test","writing_style":"","custom_rules":[],"language":"zh-CN"}"#;
        let profile: PromptProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.display_name, "砚");
        assert!(profile.avatar_emoji.is_none());
        assert_eq!(profile.persona, "test");
    }

    #[test]
    fn empty_profile_yields_empty_system_fragment() {
        assert!(PromptProfile::default()
            .to_system_prompt_fragment()
            .is_empty());
    }

    #[test]
    fn persona_fragment_includes_reply_discipline_against_self_introduction() {
        let profile = PromptProfile {
            persona: "某单位纪检监察辅助助手".into(),
            ..PromptProfile::default()
        };
        let fragment = profile.to_system_prompt_fragment();
        assert!(fragment.contains("**人格**：某单位纪检监察辅助助手"));
        assert!(fragment.contains("回复纪律"));
        assert!(fragment.contains("短问候"));
        assert!(fragment.contains("禁止主动复述人格"));
        assert!(fragment.contains("你是谁"));
    }
}
