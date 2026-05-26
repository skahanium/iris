use chrono::{DateTime, Utc};

use super::kind::VersionKind;

/// Minimum gap between `auto_idle` snapshots for the same file.
pub const AUTO_IDLE_MIN_INTERVAL_SECS: i64 = 10 * 60;
/// Do not create `auto_idle` within this many seconds of any snapshot.
pub const AUTO_IDLE_ANY_SNAPSHOT_GAP_SECS: i64 = 2 * 60;
/// Maximum `auto_idle` snapshots retained per file (oldest removed after insert).
pub const AUTO_IDLE_MAX_PER_FILE: usize = 30;

#[derive(Debug, Clone)]
pub struct LatestSnapshot {
    pub content_hash: String,
    pub kind: VersionKind,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SnapshotDecisionInput<'a> {
    pub kind: VersionKind,
    pub content_hash: &'a str,
    pub latest: Option<LatestSnapshot>,
    pub last_auto_idle_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

/// Whether a new snapshot row should be created.
pub fn should_create_snapshot(input: &SnapshotDecisionInput<'_>) -> bool {
    if input.kind.bypasses_hash_dedup() {
        return true;
    }

    let Some(latest) = &input.latest else {
        return true;
    };

    if latest.content_hash == input.content_hash {
        return false;
    }

    if input.kind == VersionKind::AutoIdle {
        if input
            .now
            .signed_duration_since(latest.created_at)
            .num_seconds()
            < AUTO_IDLE_ANY_SNAPSHOT_GAP_SECS
        {
            return false;
        }

        if let Some(last_idle) = input.last_auto_idle_at {
            if input.now.signed_duration_since(last_idle).num_seconds()
                < AUTO_IDLE_MIN_INTERVAL_SECS
            {
                return false;
            }
        }
    }

    true
}

pub fn content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn parse_created_at(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(hash: &str, kind: VersionKind, secs_ago: i64) -> LatestSnapshot {
        LatestSnapshot {
            content_hash: hash.to_string(),
            kind,
            created_at: Utc::now() - chrono::Duration::seconds(secs_ago),
        }
    }

    #[test]
    fn policy_skips_duplicate_hash_for_manual() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::Manual,
            content_hash: "abc",
            latest: Some(latest("abc", VersionKind::Manual, 60)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        assert!(!should_create_snapshot(&input));
    }

    #[test]
    fn policy_allows_manual_when_hash_changed() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::Manual,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::Manual, 60)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        assert!(should_create_snapshot(&input));
    }

    #[test]
    fn policy_pre_restore_bypasses_duplicate_hash() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::PreRestore,
            content_hash: "abc",
            latest: Some(latest("abc", VersionKind::Manual, 10)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        assert!(should_create_snapshot(&input));
    }

    #[test]
    fn policy_auto_idle_respects_two_minute_gap_after_any_snapshot() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::AutoIdle,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::Manual, 30)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        assert!(!should_create_snapshot(&input));
    }

    #[test]
    fn policy_auto_idle_respects_ten_minute_idle_interval() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::AutoIdle,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::AutoIdle, 600)),
            last_auto_idle_at: Some(Utc::now() - chrono::Duration::seconds(120)),
            now: Utc::now(),
        };
        assert!(!should_create_snapshot(&input));
    }

    #[test]
    fn policy_auto_idle_allowed_when_intervals_met() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::AutoIdle,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::Manual, 400)),
            last_auto_idle_at: Some(Utc::now() - chrono::Duration::seconds(700)),
            now: Utc::now(),
        };
        assert!(should_create_snapshot(&input));
    }
}
