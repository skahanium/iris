//! Closed baseline identity for deterministic evaluation reports.
//!
//! Reports persist only hashes, commit SHAs, and platform tokens. Prompts,
//! paths, and user names never enter this object.

use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::*;

/// Embedded agent-answer fixture bytes hashed into every deterministic report.
pub(crate) const AGENT_ANSWER_V1_FIXTURE: &str =
    include_str!("../../../../docs/eval/fixtures/agent-answer-v1.json");

pub(crate) const EVAL_SUMMARY_SCHEMA_V3: &str = "agent-eval-summary-v3";
pub(crate) const CAPACITY_REPORT_SCHEMA_V3: &str = "agent-capacity-report-v3";

const TEST_SOURCE_COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Whether the source tree that produced a report was clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkingTree {
    Clean,
    Dirty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FixtureHashes {
    agent_answer_v1: String,
}

/// Reproducible identity of one deterministic evaluation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineIdentity {
    source_commit: String,
    working_tree: WorkingTree,
    comparable: bool,
    release: String,
    scoring_schema: String,
    scenario_set_hash: String,
    fixture_hashes: FixtureHashes,
    os: String,
    arch: String,
}

impl BaselineIdentity {
    /// Synthetic identity for in-process tests. Never calls git.
    pub(crate) fn for_tests(working_tree: WorkingTree) -> Result<Self, EvalContractError> {
        Self::from_commit(TEST_SOURCE_COMMIT.to_string(), working_tree)
    }

    /// Capture HEAD and porcelain status from the workspace git directory.
    pub(crate) fn capture_from_git() -> Result<Self, EvalContractError> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| EvalContractError::new("baseline_identity_git_missing"))?;
        let commit = git_stdout(root, &["rev-parse", "HEAD"])?;
        let commit = commit.trim().to_string();
        let porcelain = git_stdout(root, &["status", "--porcelain"])?;
        let working_tree = if porcelain.trim().is_empty() {
            WorkingTree::Clean
        } else {
            WorkingTree::Dirty
        };
        Self::from_commit(commit, working_tree)
    }

    fn from_commit(
        source_commit: String,
        working_tree: WorkingTree,
    ) -> Result<Self, EvalContractError> {
        if !is_hex(&source_commit, 40) {
            return Err(EvalContractError::new("baseline_identity_commit_invalid"));
        }
        let scenario_set_hash = scenario_set_hash();
        let fixture_hash = Self::agent_answer_v1_sha256();
        if !is_hex(&scenario_set_hash, 64) || !is_hex(&fixture_hash, 64) {
            return Err(EvalContractError::new("baseline_identity_hash_invalid"));
        }
        Ok(Self {
            source_commit,
            working_tree,
            comparable: matches!(working_tree, WorkingTree::Clean),
            release: env!("CARGO_PKG_VERSION").to_string(),
            scoring_schema: CAPACITY_REPORT_SCHEMA_V3.to_string(),
            scenario_set_hash,
            fixture_hashes: FixtureHashes {
                agent_answer_v1: fixture_hash,
            },
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        })
    }

    pub(crate) const fn comparable(&self) -> bool {
        self.comparable
    }

    pub(crate) const fn working_tree(&self) -> WorkingTree {
        self.working_tree
    }

    pub(crate) fn agent_answer_v1_sha256() -> String {
        sha256_hex(AGENT_ANSWER_V1_FIXTURE.as_bytes())
    }

    pub(crate) fn fixture_hash_matches(expected: &str, observed: &str) -> bool {
        is_hex(expected, 64) && is_hex(observed, 64) && expected == observed
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn push_counted(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes());
    let take = usize::try_from(len).unwrap_or(bytes.len());
    out.extend_from_slice(&bytes[..take.min(bytes.len())]);
}

fn scenario_set_hash() -> String {
    let mut canonical = Vec::new();
    for plan in &BASE_QUESTION_PLANS {
        push_counted(&mut canonical, group_slug(plan.group).as_bytes());
        push_counted(&mut canonical, language_slug(plan.language).as_bytes());
        push_counted(&mut canonical, plan.domain.as_bytes());
        push_counted(
            &mut canonical,
            answer_mode_slug(plan.answer_mode).as_bytes(),
        );
        push_counted(&mut canonical, plan.prompt.as_bytes());
    }
    sha256_hex(&canonical)
}

fn group_slug(group: EvidenceGroup) -> &'static str {
    match group {
        EvidenceGroup::NoRetrieval => "no_retrieval",
        EvidenceGroup::LocalOnly => "local_only",
        EvidenceGroup::WebOnly => "web_only",
        EvidenceGroup::Hybrid => "hybrid",
    }
}

fn language_slug(language: ScenarioLanguage) -> &'static str {
    match language {
        ScenarioLanguage::Chinese => "chinese",
        ScenarioLanguage::English => "english",
        ScenarioLanguage::Mixed => "mixed",
    }
}

fn answer_mode_slug(mode: AnswerMode) -> &'static str {
    match mode {
        AnswerMode::EvidenceGrounded => "evidence_grounded",
        AnswerMode::Creative => "creative",
        AnswerMode::Rewrite => "rewrite",
    }
}

fn git_stdout(root: &std::path::Path, args: &[&str]) -> Result<String, EvalContractError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|_| EvalContractError::new("baseline_identity_git_missing"))?;
    if !output.status.success() {
        return Err(EvalContractError::new("baseline_identity_git_missing"));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| EvalContractError::new("baseline_identity_git_invalid"))
}
