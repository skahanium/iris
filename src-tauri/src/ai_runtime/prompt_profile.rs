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
    "鐮?.to_string()
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
            "瀛︽湳涓ヨ皑",
            PromptProfile {
                display_name: "鐮?.into(),
                avatar_emoji: Some("馃摎".into()),
                persona: "涓ヨ皑銆佸瑙傜殑瀛︽湳鍔╂墜锛岄噸瑙嗚瘉鎹笌寮曠敤銆?.into(),
                writing_style: "缁撴瀯娓呮櫚銆佹湳璇噯纭€侀伩鍏嶅彛璇寲銆?.into(),
                custom_rules: vec![
                    "浼樺厛寮曠敤涓婁笅鏂囪瘉鎹€?.into(),
                    "涓嶇‘瀹氭椂鏄庣‘璇存槑灞€闄愩€?.into(),
                ],
                language: "zh-CN".into(),
            },
        ),
        (
            "鍒涙剰鍐欎綔",
            PromptProfile {
                display_name: "鐮?.into(),
                avatar_emoji: Some("馃枊锔?.into()),
                persona: "瀵屾湁鎯宠薄鍔涚殑鍐欎綔浼欎即锛屽杽浜庢嫇灞曟儏鑺備笌浜虹墿銆?.into(),
                writing_style: "鐢熷姩銆佹湁鐢婚潰鎰燂紝閫傚害淇緸銆?.into(),
                custom_rules: vec!["淇濇寔涓庢棦鏈夎瀹氫竴鑷淬€?.into()],
                language: "zh-CN".into(),
            },
        ),
        (
            "绠€娲侀珮鏁?,
            PromptProfile {
                display_name: "鐮?.into(),
                avatar_emoji: Some("鈿?.into()),
                persona: "楂樻晥鎵ц鍨嬪姪鎵嬶紝鐩磋揪瑕佺偣銆?.into(),
                writing_style: "鐭彞銆佸垪琛ㄣ€佸皯搴熻瘽銆?.into(),
                custom_rules: vec!["榛樿涓嶈秴杩囦笁娈点€?.into()],
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
        let mut s = String::from("## 鐢ㄦ埛 AI 浜烘牸閰嶇疆\n\n");
        if !self.persona.is_empty() {
            s.push_str(&format!("**浜烘牸**锛歿}\n\n", self.persona));
        }
        if !self.writing_style.is_empty() {
            s.push_str(&format!("**鍐欎綔椋庢牸**锛歿}\n\n", self.writing_style));
        }
        if !self.language.is_empty() {
            s.push_str(&format!("**鍥炵瓟璇█**锛歿}\n\n", self.language));
        }
        if !self.custom_rules.is_empty() {
            s.push_str("**鑷畾涔夎鍒?*锛歕n");
            for rule in &self.custom_rules {
                s.push_str(&format!("- {rule}\n"));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_includes_display_name() {
        let profile = PromptProfile::default();
        assert_eq!(profile.display_name, "鐮?);
        assert!(profile.avatar_emoji.is_none());
    }

    #[test]
    fn deserializes_legacy_profile_without_display_fields() {
        let json = r#"{"persona":"test","writing_style":"","custom_rules":[],"language":"zh-CN"}"#;
        let profile: PromptProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.display_name, "鐮?);
        assert!(profile.avatar_emoji.is_none());
        assert_eq!(profile.persona, "test");
    }
}
