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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotSkipReason {
    DuplicateHash,
    AutoIdleAnySnapshotCooldown,
    AutoIdleIntervalCooldown,
}

impl SnapshotSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateHash => "duplicate_hash",
            Self::AutoIdleAnySnapshotCooldown => "auto_idle_any_snapshot_cooldown",
            Self::AutoIdleIntervalCooldown => "auto_idle_interval_cooldown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotDecision {
    pub create: bool,
    pub skip_reason: Option<SnapshotSkipReason>,
}

impl SnapshotDecision {
    fn create() -> Self {
        Self {
            create: true,
            skip_reason: None,
        }
    }

    fn skip(skip_reason: SnapshotSkipReason) -> Self {
        Self {
            create: false,
            skip_reason: Some(skip_reason),
        }
    }
}

/// Whether a new snapshot row should be created.
pub fn decide_snapshot(input: &SnapshotDecisionInput<'_>) -> SnapshotDecision {
    if input.kind.bypasses_hash_dedup() {
        return SnapshotDecision::create();
    }

    let Some(latest) = &input.latest else {
        return SnapshotDecision::create();
    };

    if latest.content_hash == input.content_hash {
        return SnapshotDecision::skip(SnapshotSkipReason::DuplicateHash);
    }

    if input.kind == VersionKind::AutoIdle {
        if input
            .now
            .signed_duration_since(latest.created_at)
            .num_seconds()
            < AUTO_IDLE_ANY_SNAPSHOT_GAP_SECS
        {
            return SnapshotDecision::skip(SnapshotSkipReason::AutoIdleAnySnapshotCooldown);
        }

        if let Some(last_idle) = input.last_auto_idle_at {
            if input.now.signed_duration_since(last_idle).num_seconds()
                < AUTO_IDLE_MIN_INTERVAL_SECS
            {
                return SnapshotDecision::skip(SnapshotSkipReason::AutoIdleIntervalCooldown);
            }
        }
    }

    SnapshotDecision::create()
}

/// Whether a new snapshot row should be created.
#[allow(dead_code)]
pub fn should_create_snapshot(input: &SnapshotDecisionInput<'_>) -> bool {
    decide_snapshot(input).create
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

    #[test]
    fn policy_reports_duplicate_hash_skip_reason() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::Manual,
            content_hash: "abc",
            latest: Some(latest("abc", VersionKind::Manual, 60)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        let decision = decide_snapshot(&input);
        assert!(!decision.create);
        assert_eq!(
            decision.skip_reason,
            Some(SnapshotSkipReason::DuplicateHash)
        );
    }

    #[test]
    fn policy_reports_auto_idle_any_snapshot_cooldown_skip_reason() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::AutoIdle,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::Manual, 30)),
            last_auto_idle_at: None,
            now: Utc::now(),
        };
        let decision = decide_snapshot(&input);
        assert!(!decision.create);
        assert_eq!(
            decision.skip_reason,
            Some(SnapshotSkipReason::AutoIdleAnySnapshotCooldown)
        );
    }

    #[test]
    fn policy_reports_auto_idle_interval_skip_reason() {
        let input = SnapshotDecisionInput {
            kind: VersionKind::AutoIdle,
            content_hash: "new",
            latest: Some(latest("old", VersionKind::AutoIdle, 600)),
            last_auto_idle_at: Some(Utc::now() - chrono::Duration::seconds(120)),
            now: Utc::now(),
        };
        let decision = decide_snapshot(&input);
        assert!(!decision.create);
        assert_eq!(
            decision.skip_reason,
            Some(SnapshotSkipReason::AutoIdleIntervalCooldown)
        );
    }
}
