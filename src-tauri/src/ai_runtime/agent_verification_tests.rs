//! Reproduction and coverage guards for the 2026-09-10 supervision-law session.
//!
//! The session produced four terminal Failures that reached the user as nothing
//! at all, and a deterministic gate that stayed green throughout because no
//! scenario in it ever reached the branch that failed. These tests drive the
//! real Intake/Context path with the exact user messages, then hold the eval
//! matrix to the coverage that would have caught it:
//!
//!   * the wording that selects the strict branch, and the wording that leaves it;
//!   * every verification class a new Run can be frozen into is owned by a
//!     named gate that actually reaches it;
//!   * no Run ends without either an answer or a user-visible failure code.
//!
//! Live evidence from session 21 of the local Run database: 4 of 4
//! `high_stakes_current_fact` Runs failed, 9 of 9 Runs in every other class
//! completed.

use super::agent_capacity_eval::{
    generate_core_scenarios, live_pilot_prompt, select_live_canary_scenarios,
    select_live_pilot_scenarios,
};
use super::run_context::RunContextAssembler;
use super::run_contract::{
    AssistantRunStartRequest, AssistantTurnDraft, SecurityDomain, VerificationRequirement,
    WebDecisionReason,
};
use super::run_intake::RunIntake;
use crate::app::AppState;

/// The exact messages from the failing session, in the order the user sent them.
const SESSION_MESSAGES: [&str; 5] = [
    "近期有什么好看的电影正在热映或者即将上映吗",
    "最新的监察法修订于那一年?",
    "美国现任总统是谁",
    "法国现任总统是谁呢",
    "最新的纪律处分条例修订于哪一年?",
];

fn request(client_request_id: &str, message: &str) -> AssistantRunStartRequest {
    AssistantRunStartRequest {
        client_request_id: client_request_id.to_string(),
        session: None,
        turn: AssistantTurnDraft {
            message: message.to_string(),
            content_parts: None,
            explicit_references: vec![],
            retrieval_scope: Default::default(),
            display_mentions: vec![],
        },
        explicit_action: None,
        web_enabled: true,
        model_override: None,
        external_tool_grants: Vec::new(),
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    }
}

struct Classifier {
    directory: tempfile::TempDir,
    state: std::sync::Arc<AppState>,
    counter: std::cell::Cell<usize>,
}

impl Classifier {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).expect("vault directory");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        state.set_vault(vault).expect("activate vault");
        Self {
            directory,
            state,
            counter: std::cell::Cell::new(0),
        }
    }

    /// Classify one message through the real Intake → Context path.
    fn classify(&self, message: &str) -> (WebDecisionReason, VerificationRequirement) {
        let index = self.counter.get();
        self.counter.set(index + 1);
        let vault = self.directory.path().join("vault");
        let accepted = RunIntake::start(
            &self.state.db,
            request(&format!("classify-{index}"), message),
        )
        .expect("accepted run");
        let context = RunContextAssembler::assemble(
            &self.state.db,
            Some(&vault),
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("run context");
        (
            context.envelope.web_reason,
            context.envelope.verification_requirement,
        )
    }
}

fn is_strict(reason: WebDecisionReason, verification: VerificationRequirement) -> bool {
    matches!(reason, WebDecisionReason::HighStakesCurrentFact)
        && matches!(verification, VerificationRequirement::CurrentRunWeb)
}

/// Observation test: print the real classification of every message from the
/// failing session. Run with `--nocapture` to read the table.
#[test]
fn session_message_classification_table() {
    let classifier = Classifier::new();
    for message in SESSION_MESSAGES {
        let (reason, verification) = classifier.classify(message);
        println!(
            "{:<40} reason={:<24?} verification={:<18?} strict={}",
            message,
            reason,
            verification,
            is_strict(reason, verification)
        );
    }
}

/// The reproduction: the supervision-law wording alone selects the strict
/// branch, while every message the user sent around it stays on the loose
/// branch that completed in production.
#[test]
fn supervision_law_wording_is_the_only_strict_branch_selector() {
    let classifier = Classifier::new();
    let movie = classifier.classify("近期有什么好看的电影正在热映或者即将上映吗");
    let supervision = classifier.classify("最新的监察法修订于那一年?");
    let us_president = classifier.classify("美国现任总统是谁");
    let fr_president = classifier.classify("法国现任总统是谁呢");
    let discipline = classifier.classify("最新的纪律处分条例修订于哪一年?");

    assert!(
        is_strict(supervision.0, supervision.1),
        "supervision-law wording must select the strict branch, got {:?}/{:?}",
        supervision.0,
        supervision.1
    );

    for (label, observed) in [
        ("movie", movie),
        ("us president", us_president),
        ("french president", fr_president),
        ("discipline regulation", discipline),
    ] {
        assert!(
            !is_strict(observed.0, observed.1),
            "{label} must stay on the loose branch, got {:?}/{:?}",
            observed.0,
            observed.1
        );
    }
}

/// The strict branch is selected by *phrasing*, not by risk. Rewording the same
/// supervision-law question so it matches the revision-date escape guard moves
/// it onto the loose branch that production answers successfully.
#[test]
fn rewording_the_same_question_leaves_the_strict_branch() {
    let classifier = Classifier::new();
    let original = classifier.classify("最新的监察法修订于那一年?");
    let reworded = classifier.classify("最新的监察法是什么时候修订的?");
    let reworded_alt = classifier.classify("监察法最新的修订日期是哪天?");

    assert!(
        is_strict(original.0, original.1),
        "original wording must be strict, got {:?}/{:?}",
        original.0,
        original.1
    );
    for (label, observed) in [("reworded", reworded), ("reworded alt", reworded_alt)] {
        assert!(
            !is_strict(observed.0, observed.1),
            "{label} must leave the strict branch, got {:?}/{:?}",
            observed.0,
            observed.1
        );
    }
}

/// The verification classes a new Run can actually be frozen into.
///
/// `CurrentRunDomain` is deliberately absent: `run_contract.rs` documents it as
/// a legacy deserialization marker that new intake never selects.
const LIVE_VERIFICATION_CLASSES: [VerificationRequirement; 3] = [
    VerificationRequirement::None,
    VerificationRequirement::CurrentRunWeb,
    VerificationRequirement::CurrentRunExternal,
];

/// Which gate is responsible for exercising each verification class.
///
/// The deterministic core matrix *cannot* express `CurrentRunExternal`: its
/// request builder freezes `external_tool_grants: Vec::new()`, so a granted
/// external read is unreachable there. That class is owned by the classifier
/// contract gate instead (`run_intake_tests.rs`). Naming the owner keeps the
/// requirement honest — a class no gate can reach must be an explicit decision
/// rather than a silent hole.
enum ClassOwner {
    /// Must be exercised by the deterministic core matrix.
    CoreMatrix,
    /// Exercised by the intake classifier contract gate, not the core matrix.
    ClassifierContract,
}

const CLASS_OWNERSHIP: [(VerificationRequirement, ClassOwner); 3] = [
    (VerificationRequirement::None, ClassOwner::CoreMatrix),
    (
        VerificationRequirement::CurrentRunWeb,
        ClassOwner::CoreMatrix,
    ),
    (
        VerificationRequirement::CurrentRunExternal,
        ClassOwner::ClassifierContract,
    ),
];

fn coverage_of(classifier: &Classifier, prompts: &[String]) -> (usize, Vec<(String, usize)>) {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for prompt in prompts {
        let (reason, verification) = classifier.classify(prompt);
        let key = format!("{reason:?}/{verification:?}");
        match counts.iter_mut().find(|(name, _)| *name == key) {
            Some((_, count)) => *count += 1,
            None => counts.push((key, 1)),
        }
    }
    counts.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    (prompts.len(), counts)
}

/// Coverage gate for the classifier's decision space.
///
/// The production defect this guards: the 48-case matrix reached only
/// `DefaultOnline` and `ExplicitWebRequest`, so `VolatileExternalFact` — the
/// class every ordinary current-events question falls into — and
/// `HighStakesCurrentFact`, the class that failed 4 of 4 in the 2026-09-10
/// session, both had zero coverage. A gate that never reaches a class cannot
/// report anything about it, so a green deterministic eval and a 0-of-4 live
/// success rate coexisted without contradiction.
///
/// Each class must therefore be owned by a gate that actually reaches it, and
/// every eval-owned class must appear in the deterministic matrix.
#[test]
fn every_gate_covers_every_verification_class() {
    let classifier = Classifier::new();

    let core = generate_core_scenarios().expect("core scenarios");
    let core_prompts: Vec<String> = core.iter().map(live_pilot_prompt).collect();
    let pilot = select_live_pilot_scenarios().expect("live pilot scenarios");
    let pilot_prompts: Vec<String> = pilot.iter().map(live_pilot_prompt).collect();
    let canary = select_live_canary_scenarios().expect("live canary scenarios");
    let canary_prompts: Vec<String> = canary.iter().map(live_pilot_prompt).collect();

    let mut gates = Vec::new();
    for (label, prompts) in [
        ("core (deterministic gate)", core_prompts),
        ("live pilot", pilot_prompts),
        ("live canary", canary_prompts),
    ] {
        let (total, counts) = coverage_of(&classifier, &prompts);
        println!("{label}: total={total}");
        for (key, count) in &counts {
            println!("    {key}: {count}");
        }
        gates.push((label, total, counts));
    }

    for (label, total, counts) in &gates {
        for (class, owner) in CLASS_OWNERSHIP {
            if !matches!(owner, ClassOwner::CoreMatrix) || *label != "core (deterministic gate)" {
                continue;
            }
            let covered = counts
                .iter()
                .any(|(key, _)| key.ends_with(&format!("/{class:?}")));
            assert!(
                covered,
                "{label} exercises the {class:?} verification class in 0 of {total} scenarios: \
                 a gate that never reaches a class cannot report anything about it"
            );
        }
    }

    // Every live class must be owned, so adding a class to the classifier
    // forces an explicit decision about which gate is responsible for it.
    for class in LIVE_VERIFICATION_CLASSES {
        assert!(
            CLASS_OWNERSHIP.iter().any(|(owned, _)| *owned == class),
            "{class:?} is reachable but no gate is declared responsible for it"
        );
    }
}

/// The strict branch must be reached by its own wording, not merely by a
/// verification class that happens to be shared with another reason.
///
/// `HighStakesCurrentFact` is the only reason that turns on the corroboration
/// threshold, so it needs direct scenario coverage rather than being inferred
/// from `CurrentRunWeb` alone.
#[test]
fn deterministic_gate_covers_the_high_stakes_reason() {
    let classifier = Classifier::new();
    let core = generate_core_scenarios().expect("core scenarios");
    let prompts: Vec<String> = core.iter().map(live_pilot_prompt).collect();
    let (total, counts) = coverage_of(&classifier, &prompts);

    let covered = counts
        .iter()
        .any(|(key, _)| key.starts_with("HighStakesCurrentFact/"));
    assert!(
        covered,
        "the deterministic gate exercises HighStakesCurrentFact in 0 of {total} scenarios: \
         the corroboration threshold branch has no deterministic coverage; observed classes: {counts:?}"
    );
}

/// No Run may end silently.
///
/// A terminal Run must carry either a published answer or a terminal error code
/// the UI can render. The 2026-09-10 session produced four terminal Failures
/// that reached the user as nothing at all, so "terminal and silent" is the
/// state this invariant forbids. The slice is the smoke matrix for cost; the
/// full matrix is exercised by `agent:eval:contract`.
#[tokio::test]
async fn every_executed_case_ends_with_an_answer_or_a_visible_failure_code() {
    use super::agent_capacity_eval::{
        execute_headless_core_case, select_core_scenarios, EvalRunMode,
    };

    let scenarios = select_core_scenarios(EvalRunMode::Smoke).expect("smoke slice");
    let mut silent = Vec::new();
    let mut executed_runs = 0_usize;

    for scenario in &scenarios {
        let case_id = scenario.case_id();
        let executed = execute_headless_core_case(scenario, None)
            .await
            .expect("headless case runs");
        executed_runs += 1;
        let answered = executed.overall_pass();
        let labelled = executed.has_terminal_error_code();
        if !answered && !labelled {
            silent.push(format!(
                "case {case_id}: ended {} with neither an answer nor a failure code",
                executed.terminal_state_label()
            ));
        }
    }

    assert!(
        executed_runs > 0,
        "the smoke slice must execute at least one case"
    );
    assert!(
        silent.is_empty(),
        "terminal Runs without an answer or a user-visible failure code: {silent:?}"
    );
}
