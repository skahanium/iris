use super::*;

#[test]
fn review_regression_a_budget_phase_recovery_retains_unknown_attempt_and_idempotent_settle() {
    use crate::ai_runtime::agent_tool_loop::AgentModelTurnBudget;
    use crate::ai_runtime::model_turn_ledger::{
        self as ledger, AttemptPurpose, BindGuard, BudgetPhase,
    };
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key);
    input.run_id = "review-phase-recovery".into();
    let accepted = AgentRunRepository::accept(&db, input).unwrap();
    let mut policy = RunBudgetPolicy::standard();
    policy.max_model_turns = 3;
    policy.post_confirmation_max_model_turns = 2;
    let budget = AgentModelTurnBudget {
        max_prompt_tokens: Some(100),
        max_completion_tokens: Some(policy.max_completion_tokens),
        max_turn_output_tokens: Some(100),
    };
    {
        let _guard =
            BindGuard::persisted(&db, &accepted.run_id, BudgetPhase::Main, &policy).unwrap();
        ledger::claim_attempt(Some(&db), &accepted.run_id, AttemptPurpose::Model, budget).unwrap();
    }
    {
        let _guard =
            BindGuard::persisted(&db, &accepted.run_id, BudgetPhase::Main, &policy).unwrap();
        assert_eq!(ledger::used(&accepted.run_id), 1);
        assert_eq!(ledger::accounting(&accepted.run_id).2, 100);
        let lease =
            ledger::claim_attempt(Some(&db), &accepted.run_id, AttemptPurpose::Model, budget)
                .unwrap();
        let usage = crate::ai_types::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            ..Default::default()
        };
        ledger::settle_attempt(Some(&db), &lease, Some(&usage)).unwrap();
        ledger::settle_attempt(Some(&db), &lease, Some(&usage)).unwrap();
        assert_eq!(ledger::accounting(&accepted.run_id), (2, 110, 120));
        assert!(
            ledger::claim_attempt(Some(&db), &accepted.run_id, AttemptPurpose::Retry, budget)
                .is_err()
        );
    }
    {
        let _guard = BindGuard::persisted(
            &db,
            &accepted.run_id,
            BudgetPhase::PostConfirmation,
            &policy,
        )
        .unwrap();
        assert_eq!(ledger::used(&accepted.run_id), 0);
        ledger::claim_attempt(Some(&db), &accepted.run_id, AttemptPurpose::Model, budget).unwrap();
    }
    let _guard = BindGuard::persisted(
        &db,
        &accepted.run_id,
        BudgetPhase::PostConfirmation,
        &policy,
    )
    .unwrap();
    assert_eq!(ledger::used(&accepted.run_id), 1);
    assert!(
        ledger::claim_attempt(Some(&db), &accepted.run_id, AttemptPurpose::Retry, budget).is_err()
    );
    ledger::claim_attempt(
        Some(&db),
        &accepted.run_id,
        AttemptPurpose::FinalSynthesis,
        budget,
    )
    .unwrap();
    let (stored, _) = AgentRunRepository::model_budget_snapshot(&db, &accepted.run_id).unwrap();
    assert_eq!(stored.unwrap()["root_used"], 4);
}

#[test]
fn review_regression_a_legacy_running_budget_cannot_be_recreated_as_unused() {
    use crate::ai_runtime::model_turn_ledger::{
        self as ledger, AttemptPurpose, BindGuard, BudgetPhase,
    };
    let (db, session_id, session_key) = setup();
    let mut input = accept_input(session_id, session_key);
    input.run_id = "review-legacy-budget".into();
    let accepted = AgentRunRepository::accept(&db, input).unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE agent_runs SET status='running' WHERE run_id=?1",
            [&accepted.run_id],
        )?;
        Ok(())
    })
    .unwrap();
    let _guard = BindGuard::persisted(
        &db,
        &accepted.run_id,
        BudgetPhase::PostConfirmation,
        &RunBudgetPolicy::standard(),
    )
    .unwrap();
    assert!(ledger::claim_attempt(
        Some(&db),
        &accepted.run_id,
        AttemptPurpose::FinalSynthesis,
        Default::default()
    )
    .is_err());
}
