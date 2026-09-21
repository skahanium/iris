//! Atomic, hierarchical model-attempt accounting. Every dispatched attempt is
//! charged to its child (when present), its phase, and the root Run. Persisted
//! reservations survive process loss; only a known settlement releases tokens.

use crate::ai_runtime::agent_run_repository::AgentRunRepository;
use crate::ai_runtime::agent_tool_loop::{parent_run_id_for_provider_scope, AgentModelTurnBudget};
use crate::ai_runtime::run_contract::{RunBudgetPolicy, SafeRunErrorCode};
use crate::ai_types::TokenUsage;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum AttemptPurpose {
    Model,
    FinalSynthesis,
    Compaction,
    NativeSearch,
    Retry,
}

tokio::task_local! {
    static PURPOSE: AttemptPurpose;
    static SCOPE: String;
}
pub(crate) fn current_scope(root_run_id: &str) -> String {
    SCOPE
        .try_with(Clone::clone)
        .unwrap_or_else(|_| root_run_id.to_string())
}
pub(crate) async fn with_scope<F: std::future::Future>(scope: &str, future: F) -> F::Output {
    SCOPE.scope(scope.to_string(), future).await
}

pub(crate) fn current_purpose() -> AttemptPurpose {
    PURPOSE
        .try_with(|purpose| *purpose)
        .unwrap_or(AttemptPurpose::Model)
}

pub(crate) async fn with_purpose<F: std::future::Future>(
    purpose: AttemptPurpose,
    future: F,
) -> F::Output {
    PURPOSE.scope(purpose, future).await
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(crate) enum BudgetPhase {
    Main,
    PostConfirmation,
}

#[derive(Clone, Serialize, Deserialize)]
struct Allowance {
    max: u32,
    used: u32,
    completion_limit: u32,
    completion_charged: u32,
    prompt_charged: u32,
    reserve_turns: u32,
    reserve_completion: u32,
}
impl Allowance {
    fn new(max: u32) -> Self {
        Self {
            max,
            used: 0,
            completion_limit: u32::MAX,
            completion_charged: 0,
            prompt_charged: 0,
            reserve_turns: 0,
            reserve_completion: 0,
        }
    }
    fn remaining(&self) -> u32 {
        self.max.saturating_sub(self.used)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ChildAllowance {
    phase: BudgetPhase,
    allowance: Allowance,
}

#[derive(Clone, Serialize, Deserialize)]
struct AttemptRecord {
    id: u32,
    scope_id: String,
    phase: BudgetPhase,
    purpose: AttemptPurpose,
    reserved_completion: u32,
    reserved_prompt: u32,
    dispatched: bool,
    settled: bool,
    usage_known: bool,
    #[serde(default)]
    reported_prompt_tokens: Option<u32>,
    #[serde(default)]
    reported_completion_tokens: Option<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LedgerSnapshot {
    schema_version: u32,
    revision: u64,
    root_max: u32,
    root_used: u32,
    active_phase: BudgetPhase,
    phases: HashMap<BudgetPhase, Allowance>,
    children: HashMap<String, ChildAllowance>,
    attempts: Vec<AttemptRecord>,
}

fn slots() -> &'static Mutex<HashMap<String, LedgerSnapshot>> {
    static SLOTS: OnceLock<Mutex<HashMap<String, LedgerSnapshot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn lock_slots() -> std::sync::MutexGuard<'static, HashMap<String, LedgerSnapshot>> {
    slots().lock().unwrap_or_else(|error| error.into_inner())
}
fn limited() -> AppError {
    AppError::run(SafeRunErrorCode::ToolLoopLimit)
}

/// Compatibility binding for standalone loops and tests; nested child scopes
/// get an independent local counter and retain their parent's phase identity.
pub(crate) fn bind(run_id: &str, max: u32) -> bool {
    let root = parent_run_id_for_provider_scope(run_id);
    let mut slots = lock_slots();
    if root != run_id {
        if let Some(slot) = slots.get_mut(root) {
            slot.children
                .entry(run_id.to_string())
                .or_insert_with(|| ChildAllowance {
                    phase: slot.active_phase,
                    allowance: Allowance::new(max),
                });
        }
        return false;
    }
    if slots.contains_key(root) {
        return false;
    }
    slots.insert(
        root.to_string(),
        LedgerSnapshot {
            schema_version: 1,
            revision: 0,
            root_max: max,
            root_used: 0,
            active_phase: BudgetPhase::Main,
            phases: HashMap::from([(BudgetPhase::Main, Allowance::new(max))]),
            children: HashMap::new(),
            attempts: Vec::new(),
        },
    );
    true
}

pub(crate) fn unbind(run_id: &str) {
    lock_slots().remove(run_id);
}

pub(crate) struct BindGuard {
    run_id: String,
    owns: bool,
}
impl BindGuard {
    pub(crate) fn new(run_id: impl Into<String>, max: u32) -> Self {
        let run_id = run_id.into();
        let owns = bind(&run_id, max);
        Self { run_id, owns }
    }

    /// Restore one authoritative root ledger before dispatch. Legacy running
    /// records without a trustworthy ledger cannot acquire fresh model quota.
    pub(crate) fn persisted(
        db: &Database,
        run_id: &str,
        phase: BudgetPhase,
        policy: &RunBudgetPolicy,
    ) -> AppResult<Self> {
        let mut slots = lock_slots();
        if slots.contains_key(run_id) {
            return Err(limited());
        }
        let (stored, fresh) = AgentRunRepository::model_budget_snapshot(db, run_id)?;
        let mut slot = if let Some(stored) = stored {
            let slot: LedgerSnapshot = serde_json::from_value(stored)?;
            if slot.schema_version != 1 {
                return Err(limited());
            }
            slot
        } else {
            let mut main = Allowance::new(policy.max_model_turns);
            let mut post = Allowance::new(policy.post_confirmation_max_model_turns);
            for allowance in [&mut main, &mut post] {
                allowance.completion_limit = policy.max_completion_tokens;
                allowance.reserve_turns = u32::from(allowance.max > 0);
                allowance.reserve_completion = policy
                    .max_turn_output_tokens
                    .min(policy.max_completion_tokens);
                if !fresh {
                    allowance.used = allowance.max;
                    allowance.completion_charged = allowance.completion_limit;
                }
            }
            LedgerSnapshot {
                schema_version: 1,
                revision: 0,
                root_max: policy
                    .max_model_turns
                    .saturating_add(policy.post_confirmation_max_model_turns),
                root_used: if fresh {
                    0
                } else {
                    policy
                        .max_model_turns
                        .saturating_add(policy.post_confirmation_max_model_turns)
                },
                active_phase: phase,
                phases: HashMap::from([
                    (BudgetPhase::Main, main),
                    (BudgetPhase::PostConfirmation, post),
                ]),
                children: HashMap::new(),
                attempts: Vec::new(),
            }
        };
        slot.active_phase = phase;
        persist(db, run_id, &mut slot)?;
        slots.insert(run_id.to_string(), slot);
        Ok(Self {
            run_id: run_id.to_string(),
            owns: true,
        })
    }
}
impl Drop for BindGuard {
    fn drop(&mut self) {
        if self.owns {
            unbind(&self.run_id);
        }
    }
}

/// Set token limits once for a standalone loop or child. Limits can only
/// tighten, never refresh already consumed allowances.
pub(crate) fn configure_scope(
    scope: &str,
    budget: AgentModelTurnBudget,
    reserve: bool,
) -> AppResult<()> {
    let mut slots = lock_slots();
    let root = parent_run_id_for_provider_scope(scope);
    let slot = slots.get_mut(root).ok_or_else(limited)?;
    let a = if root == scope {
        slot.phases
            .get_mut(&slot.active_phase)
            .ok_or_else(limited)?
    } else {
        &mut slot.children.get_mut(scope).ok_or_else(limited)?.allowance
    };
    a.completion_limit = a
        .completion_limit
        .min(budget.max_completion_tokens.unwrap_or(u32::MAX));
    if reserve {
        a.reserve_turns = u32::from(a.max > 0);
        a.reserve_completion = budget
            .max_turn_output_tokens
            .unwrap_or(0)
            .min(a.completion_limit);
    }
    Ok(())
}

fn persist(db: &Database, root: &str, slot: &mut LedgerSnapshot) -> AppResult<()> {
    let prior_revision = slot.revision;
    slot.revision = prior_revision.saturating_add(1);
    AgentRunRepository::persist_model_budget_snapshot(
        db,
        root,
        prior_revision,
        serde_json::to_value(&*slot)?,
    )
}

#[derive(Debug)]
pub(crate) struct AttemptLease {
    pub(crate) root_run_id: String,
    pub(crate) budget: AgentModelTurnBudget,
    pub(crate) id: u32,
}

/// Atomically reserve a real attempt before dispatch; retry/internal/child work
/// may not consume the root phase's final synthesis reservation.
pub(crate) fn claim_attempt(
    db: Option<&Database>,
    scope: &str,
    purpose: AttemptPurpose,
    budget: AgentModelTurnBudget,
) -> AppResult<AttemptLease> {
    let root = parent_run_id_for_provider_scope(scope);
    let mut slots = lock_slots();
    let stored = slots.get_mut(root).ok_or_else(limited)?;
    let mut slot = stored.clone();
    let phase = if root == scope {
        slot.active_phase
    } else {
        slot.children.get(scope).ok_or_else(limited)?.phase
    };
    let final_turn = purpose == AttemptPurpose::FinalSynthesis && root == scope;
    let a = slot.phases.get(&phase).ok_or_else(limited)?;
    if slot.root_used >= slot.root_max
        || a.remaining() <= if final_turn { 0 } else { a.reserve_turns }
    {
        return Err(limited());
    }
    let mut output = budget.max_turn_output_tokens.unwrap_or(0);
    if output == 0 {
        return Err(limited());
    }
    output = output.min(
        a.completion_limit
            .saturating_sub(a.completion_charged)
            .saturating_sub(if final_turn { 0 } else { a.reserve_completion }),
    );
    if root != scope {
        let child = &slot.children.get(scope).ok_or_else(limited)?.allowance;
        if child.remaining() == 0 {
            return Err(limited());
        }
        output = output.min(
            child
                .completion_limit
                .saturating_sub(child.completion_charged),
        );
    }
    if output == 0 {
        return Err(limited());
    }
    let prompt = budget.max_prompt_tokens.unwrap_or(0);
    let a = slot.phases.get_mut(&phase).ok_or_else(limited)?;
    a.used = a.used.saturating_add(1);
    a.completion_charged = a.completion_charged.saturating_add(output);
    a.prompt_charged = a.prompt_charged.saturating_add(prompt);
    if let Some(child) = slot.children.get_mut(scope) {
        child.allowance.used = child.allowance.used.saturating_add(1);
        child.allowance.completion_charged =
            child.allowance.completion_charged.saturating_add(output);
        child.allowance.prompt_charged = child.allowance.prompt_charged.saturating_add(prompt);
    }
    slot.root_used = slot.root_used.saturating_add(1);
    let id = slot.root_used;
    slot.attempts.push(AttemptRecord {
        id,
        scope_id: scope.to_string(),
        phase,
        purpose,
        reserved_completion: output,
        reserved_prompt: prompt,
        dispatched: false,
        settled: false,
        usage_known: false,
        reported_prompt_tokens: None,
        reported_completion_tokens: None,
    });
    if let Some(db) = db {
        persist(db, root, &mut slot)?;
    }
    *stored = slot;
    Ok(AttemptLease {
        root_run_id: root.to_string(),
        id,
        budget: AgentModelTurnBudget {
            max_turn_output_tokens: Some(output),
            max_completion_tokens: Some(output),
            ..budget
        },
    })
}

/// Persist the actual dispatch boundary only when its future is polled.
/// A cancelled, unpolled claim remains charged but never becomes a sent fact.
pub(crate) fn mark_dispatched(db: Option<&Database>, lease: &AttemptLease) -> AppResult<()> {
    let mut slots = lock_slots();
    let stored = slots.get_mut(&lease.root_run_id).ok_or_else(limited)?;
    let mut slot = stored.clone();
    let record = slot
        .attempts
        .iter_mut()
        .find(|record| record.id == lease.id)
        .ok_or_else(limited)?;
    if record.dispatched {
        return Ok(());
    }
    if record.settled {
        return Err(limited());
    }
    record.dispatched = true;
    if let Some(db) = db {
        persist(db, &lease.root_run_id, &mut slot)?;
    }
    *stored = slot;
    Ok(())
}

/// Project native-search attempt usage from the authoritative ledger. Known
/// subtotals remain visible even if another attempt has unknown usage.
pub(crate) fn usage_for_attempts(root: &str, ids: &[u32]) -> (Option<u32>, Option<u32>, bool) {
    if ids.is_empty() {
        return (Some(0), Some(0), false);
    }
    let slots = lock_slots();
    let Some(slot) = slots.get(root) else {
        return (None, None, true);
    };
    let mut seen = std::collections::HashSet::new();
    let mut prompt_known = false;
    let mut completion_known = false;
    let mut unknown = false;
    let mut prompt = 0_u32;
    let mut completion = 0_u32;
    for id in ids.iter().filter(|id| seen.insert(**id)) {
        match slot.attempts.iter().find(|attempt| attempt.id == *id) {
            Some(record) => {
                if let Some(input) = record.reported_prompt_tokens {
                    prompt_known = true;
                    prompt = prompt.saturating_add(input);
                } else {
                    unknown = true;
                }
                if let Some(output) = record.reported_completion_tokens {
                    completion_known = true;
                    completion = completion.saturating_add(output);
                } else {
                    unknown = true;
                }
            }
            None => unknown = true,
        }
    }
    (
        prompt_known.then_some(prompt),
        completion_known.then_some(completion),
        unknown,
    )
}

/// Identity of the actual model attempt that most recently produced actions in
/// this scope. Auxiliary/native requests cannot change their initiating turn.
pub(crate) fn latest_model_attempt(scope: &str) -> Option<u32> {
    let slots = lock_slots();
    slots
        .get(parent_run_id_for_provider_scope(scope))?
        .attempts
        .iter()
        .rev()
        .find(|attempt| {
            attempt.scope_id == scope
                && attempt.dispatched
                && matches!(
                    attempt.purpose,
                    AttemptPurpose::Model | AttemptPurpose::FinalSynthesis | AttemptPurpose::Retry
                )
        })
        .map(|attempt| attempt.id)
}

/// Settle once. Unknown usage retains its full reservation, including after a
/// process crash; duplicate settlement cannot refund or debit a second time.
pub(crate) fn settle_attempt(
    db: Option<&Database>,
    lease: &AttemptLease,
    usage: Option<&TokenUsage>,
) -> AppResult<()> {
    let mut slots = lock_slots();
    let stored = slots.get_mut(&lease.root_run_id).ok_or_else(limited)?;
    let mut slot = stored.clone();
    let record = slot
        .attempts
        .iter_mut()
        .find(|a| a.id == lease.id)
        .ok_or_else(limited)?;
    if record.settled {
        return Ok(());
    }
    record.settled = true;
    // Existing provider usage represents absent dimensions as zero. Until the
    // wire type preserves presence, zero must retain that dimension's reservation.
    let prompt = usage.map(|u| u.prompt_tokens).filter(|tokens| *tokens > 0);
    let output = usage
        .map(|u| u.completion_tokens)
        .filter(|tokens| *tokens > 0);
    record.usage_known = prompt.is_some() && output.is_some();
    record.reported_prompt_tokens = prompt;
    record.reported_completion_tokens = output;
    let adjust = |a: &mut Allowance| {
        if let Some(output) = output {
            a.completion_charged = a
                .completion_charged
                .saturating_sub(record.reserved_completion)
                .saturating_add(output);
        }
        if let Some(prompt) = prompt {
            a.prompt_charged = a
                .prompt_charged
                .saturating_sub(record.reserved_prompt)
                .saturating_add(prompt);
        }
    };
    adjust(slot.phases.get_mut(&record.phase).ok_or_else(limited)?);
    if let Some(child) = slot.children.get_mut(&record.scope_id) {
        adjust(&mut child.allowance);
    }
    if let Some(db) = db {
        persist(db, &lease.root_run_id, &mut slot)?;
    }
    *stored = slot;
    Ok(())
}

/// Count-only fixture seam. Every production dispatch uses claim_attempt.
#[cfg(test)]
pub(crate) fn claim(scope: &str) -> AppResult<u32> {
    let root = parent_run_id_for_provider_scope(scope);
    let mut slots = lock_slots();
    let slot = slots.get_mut(root).ok_or_else(limited)?;
    let phase = if root == scope {
        slot.active_phase
    } else {
        slot.children.get(scope).ok_or_else(limited)?.phase
    };
    let a = slot.phases.get(&phase).ok_or_else(limited)?;
    if slot.root_used >= slot.root_max || a.used >= a.max {
        return Err(limited());
    }
    if let Some(child) = slot.children.get(scope) {
        if child.allowance.used >= child.allowance.max {
            return Err(limited());
        }
    }
    slot.root_used = slot.root_used.saturating_add(1);
    let a = slot.phases.get_mut(&phase).ok_or_else(limited)?;
    a.used = a.used.saturating_add(1);
    let mut used = a.used;
    if let Some(child) = slot.children.get_mut(scope) {
        child.allowance.used += 1;
        used = child.allowance.used;
    }
    Ok(used)
}

pub(crate) fn used(scope: &str) -> u32 {
    accounting(scope).0
}
pub(crate) fn accounting(scope: &str) -> (u32, u32, u32) {
    let slots = lock_slots();
    let root = parent_run_id_for_provider_scope(scope);
    let Some(slot) = slots.get(root) else {
        return (0, 0, 0);
    };
    let allowance = if root == scope {
        slot.phases.get(&slot.active_phase)
    } else {
        slot.children.get(scope).map(|c| &c.allowance)
    };
    allowance
        .map(|a| (a.used, a.prompt_charged, a.completion_charged))
        .unwrap_or_default()
}
pub(crate) fn remaining(scope: &str) -> u32 {
    let slots = lock_slots();
    let root = parent_run_id_for_provider_scope(scope);
    let Some(slot) = slots.get(root) else {
        return 0;
    };
    let phase = if root == scope {
        slot.active_phase
    } else {
        match slot.children.get(scope) {
            Some(child) => child.phase,
            None => return 0,
        }
    };
    let Some(a) = slot.phases.get(&phase) else {
        return 0;
    };
    let remaining = a
        .remaining()
        .min(slot.root_max.saturating_sub(slot.root_used));
    let output_remaining = a.completion_limit.saturating_sub(a.completion_charged);
    if output_remaining == 0 {
        return 0;
    }
    if root == scope {
        if output_remaining <= a.reserve_completion {
            remaining.min(a.reserve_turns)
        } else {
            remaining
        }
    } else {
        let child = &slot.children[scope].allowance;
        if output_remaining <= a.reserve_completion
            || child.completion_charged >= child.completion_limit
        {
            return 0;
        }
        remaining
            .saturating_sub(a.reserve_turns)
            .min(child.remaining())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_regression_a_child_budget_counts_locally_and_debits_parent() {
        let root = "review-child-root";
        let child = crate::ai_runtime::agent_tool_loop::scoped_child_provider_run_id(root, "child");
        let _root = BindGuard::new(root, 8);
        claim(root).unwrap();
        claim(root).unwrap();
        let _child = BindGuard::new(&child, 2);
        assert_eq!(used(&child), 0);
        assert_eq!(claim(&child).unwrap(), 1);
        assert_eq!(used(root), 3, "child attempt must debit parent phase");
    }

    #[test]
    fn review_regression_a_unbound_attempt_fails_closed() {
        assert!(claim("review-unbound").is_err());
    }

    #[test]
    fn unbound_claim_is_denied() {
        let run_id = "ledger-unbound";
        unbind(run_id);
        assert!(claim(run_id).is_err());
        assert_eq!(used(run_id), 0);
    }

    #[test]
    fn claims_count_retries_until_the_ceiling() {
        let run_id = "ledger-retry";
        let _guard = BindGuard::new(run_id, 2);
        assert_eq!(claim(run_id).expect("first http"), 1);
        assert_eq!(claim(run_id).expect("retry http"), 2);
        assert!(claim(run_id).is_err(), "third attempt must not send");
        assert_eq!(used(run_id), 2);
    }

    #[test]
    fn drop_unbinds_so_a_later_run_is_not_charged() {
        let run_id = "ledger-drop";
        {
            let _guard = BindGuard::new(run_id, 1);
            assert_eq!(claim(run_id).expect("bound"), 1);
        }
        assert_eq!(used(run_id), 0);
        assert!(claim(run_id).is_err());
    }

    #[test]
    fn nested_bind_does_not_reset_or_unbind_the_parent_slot() {
        let run_id = "ledger-nested";
        let parent = BindGuard::new(run_id, 4);
        assert_eq!(claim(run_id).expect("parent http"), 1);
        {
            let _child = BindGuard::new(run_id, 4);
            assert_eq!(claim(run_id).expect("child http"), 2);
        }
        assert_eq!(used(run_id), 2);
        drop(parent);
        assert_eq!(used(run_id), 0);
    }
    fn review_budget() -> AgentModelTurnBudget {
        AgentModelTurnBudget {
            max_prompt_tokens: Some(100),
            max_completion_tokens: Some(400),
            max_turn_output_tokens: Some(100),
        }
    }

    #[test]
    fn review_regression_a_concurrent_children_share_phase_ceiling_and_reserve() {
        let root = "review-parallel-root";
        let _root = BindGuard::new(root, 8);
        configure_scope(
            root,
            AgentModelTurnBudget {
                max_completion_tokens: Some(1000),
                ..review_budget()
            },
            true,
        )
        .unwrap();
        let successes = std::thread::scope(|threads| {
            let handles = (0..8)
                .map(|i| {
                    threads.spawn(move || {
                        let child =
                            crate::ai_runtime::agent_tool_loop::scoped_child_provider_run_id(
                                root,
                                &i.to_string(),
                            );
                        let _child = BindGuard::new(&child, 2);
                        claim_attempt(None, &child, AttemptPurpose::Model, review_budget()).is_ok()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|success| *success)
                .count()
        });
        assert_eq!(successes, 7);
        assert_eq!(used(root), 7);
        assert!(claim_attempt(None, root, AttemptPurpose::Retry, review_budget()).is_err());
        assert!(claim_attempt(None, root, AttemptPurpose::NativeSearch, review_budget()).is_err());
        assert!(claim_attempt(None, root, AttemptPurpose::FinalSynthesis, review_budget()).is_ok());
        assert_eq!(used(root), 8);
    }

    #[test]
    fn review_regression_a_unknown_usage_keeps_reservation_and_settlement_is_idempotent() {
        let root = "review-unknown-usage";
        let _root = BindGuard::new(root, 6);
        configure_scope(root, review_budget(), true).unwrap();
        let lease = claim_attempt(None, root, AttemptPurpose::Model, review_budget()).unwrap();
        settle_attempt(None, &lease, None).unwrap();
        assert_eq!(accounting(root), (1, 100, 100));
        settle_attempt(
            None,
            &lease,
            Some(&TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(
            accounting(root),
            (1, 100, 100),
            "unknown final settlement cannot be refunded by a duplicate"
        );
        let lease = claim_attempt(None, root, AttemptPurpose::Model, review_budget()).unwrap();
        let known = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            ..Default::default()
        };
        settle_attempt(None, &lease, Some(&known)).unwrap();
        settle_attempt(None, &lease, Some(&known)).unwrap();
        assert_eq!(accounting(root), (2, 110, 120));
    }

    #[test]
    fn review_regression_a_completion_reserve_is_unavailable_to_internal_work() {
        let root = "review-output-reserve";
        let _root = BindGuard::new(root, 10);
        configure_scope(
            root,
            AgentModelTurnBudget {
                max_completion_tokens: Some(200),
                ..review_budget()
            },
            true,
        )
        .unwrap();
        claim_attempt(None, root, AttemptPurpose::Compaction, review_budget()).unwrap();
        assert!(claim_attempt(None, root, AttemptPurpose::NativeSearch, review_budget()).is_err());
        assert!(claim_attempt(None, root, AttemptPurpose::Retry, review_budget()).is_err());
        let final_turn =
            claim_attempt(None, root, AttemptPurpose::FinalSynthesis, review_budget()).unwrap();
        assert_eq!(final_turn.budget.max_turn_output_tokens, Some(100));
    }
    #[test]
    fn review_regression_a_claim_does_not_report_dispatch_before_transport_poll() {
        let root = "review-claim-before-poll";
        let _guard = BindGuard::new(root, 2);
        let lease = claim_attempt(None, root, AttemptPurpose::Model, review_budget()).unwrap();
        assert!(!lock_slots()[root].attempts[0].dispatched);
        mark_dispatched(None, &lease).unwrap();
        mark_dispatched(None, &lease).unwrap();
        assert!(lock_slots()[root].attempts[0].dispatched);
        assert_eq!(
            used(root),
            1,
            "unpolled reservations are still charged conservatively"
        );
    }

    #[test]
    fn review_regression_a_attempt_usage_projection_preserves_unknown_and_model_origin() {
        let root = "review-attempt-projection";
        let _guard = BindGuard::new(root, 5);
        let model = claim_attempt(None, root, AttemptPurpose::Model, review_budget()).unwrap();
        assert_eq!(latest_model_attempt(root), None);
        mark_dispatched(None, &model).unwrap();
        let native =
            claim_attempt(None, root, AttemptPurpose::NativeSearch, review_budget()).unwrap();
        mark_dispatched(None, &native).unwrap();
        settle_attempt(
            None,
            &native,
            Some(&TokenUsage {
                prompt_tokens: 11,
                completion_tokens: 7,
                total_tokens: 18,
                ..Default::default()
            }),
        )
        .unwrap();
        let retry =
            claim_attempt(None, root, AttemptPurpose::NativeSearch, review_budget()).unwrap();
        mark_dispatched(None, &retry).unwrap();
        settle_attempt(None, &retry, None).unwrap();
        assert_eq!(
            usage_for_attempts(root, &[native.id, retry.id, native.id]),
            (Some(11), Some(7), true)
        );
        assert_eq!(latest_model_attempt(root), Some(model.id));
    }

    #[test]
    fn review_regression_a_partial_usage_keeps_unknown_completion_reservation() {
        let root = "review-partial-usage";
        let _guard = BindGuard::new(root, 3);
        let lease = claim_attempt(None, root, AttemptPurpose::Model, review_budget()).unwrap();
        settle_attempt(
            None,
            &lease,
            Some(&TokenUsage {
                prompt_tokens: 11,
                total_tokens: 11,
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(
            accounting(root),
            (1, 11, 100),
            "missing output usage is not a known zero"
        );
        assert_eq!(
            usage_for_attempts(root, &[lease.id]),
            (Some(11), None, true)
        );
    }
    #[test]
    fn review_regression_a_completion_exhaustion_exposes_only_reserved_final_turn() {
        let root = "review-completion-final-projection";
        let _guard = BindGuard::new(root, 4);
        let mut budget = review_budget();
        budget.max_completion_tokens = Some(200);
        configure_scope(root, budget, true).unwrap();
        claim_attempt(None, root, AttemptPurpose::Model, budget).unwrap();
        assert_eq!(remaining(root), 1);
        claim_attempt(None, root, AttemptPurpose::FinalSynthesis, budget).unwrap();
        assert_eq!(remaining(root), 0);
    }
}
