use serde::Serialize;

/// Snapshot origin; stored in `versions.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    AutoIdle,
    Manual,
    PreRestore,
    Finalize,
    PreClose,
}

impl VersionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoIdle => "auto_idle",
            Self::Manual => "manual",
            Self::PreRestore => "pre_restore",
            Self::Finalize => "finalize",
            Self::PreClose => "pre_close",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto_idle" => Some(Self::AutoIdle),
            "manual" => Some(Self::Manual),
            "pre_restore" => Some(Self::PreRestore),
            "finalize" => Some(Self::Finalize),
            "pre_close" => Some(Self::PreClose),
            _ => None,
        }
    }

    /// Kinds that must record even when content matches the latest snapshot.
    pub fn bypasses_hash_dedup(self) -> bool {
        matches!(self, Self::PreRestore | Self::Finalize)
    }
}
