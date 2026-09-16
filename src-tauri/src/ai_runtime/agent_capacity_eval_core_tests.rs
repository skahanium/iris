//! Deterministic baseline-identity, headless-core and pressure-plan tests.
//!
//! Split out of `agent_capacity_eval_tests.rs` to keep that file inside its
//! pinned size budget. These tests share the fixture builders in
//! `agent_capacity_eval_test_support`.
//!
//! The import list is the superset this block used inside the parent test
//! module.
use super::agent_capacity_eval::{
    build_agent_capacity_report, calculate_stable_boundary, execute_headless_core_case,
    execute_pressure_staircases, execute_smoke_continuity_and_tool_boundaries,
    generate_core_scenarios, generate_pressure_staircases, report_gate_plan_count,
    run_combined_terminal_cases, run_hard_boundary_probes, run_headless_core_evaluation,
    run_security_track, serialize_agent_capacity_report, serialize_evaluation_summary,
    validate_serialized_evaluation_summary, write_blind_review_packet, BaselineIdentity,
    CheckStatus, EvalFault, EvalRunMode, EvidenceGroup, ImplicitVaultExpectation,
    PressureDimension, StableLevelObservation, WebState, WorkingTree, AGENT_ANSWER_V1_FIXTURE,
};

#[test]
fn baseline_identity_marks_dirty_tree_not_comparable() {
    let clean = BaselineIdentity::for_tests(WorkingTree::Clean).expect("clean identity");
    assert!(clean.comparable());
    assert_eq!(clean.working_tree(), WorkingTree::Clean);
    let dirty = BaselineIdentity::for_tests(WorkingTree::Dirty).expect("dirty identity");
    assert!(
        !dirty.comparable(),
        "a dirty working tree must not be a comparable frozen baseline"
    );
}

#[test]
fn baseline_identity_rejects_tampered_fixture_hash() {
    let expected = BaselineIdentity::agent_answer_v1_sha256();
    assert!(BaselineIdentity::fixture_hash_matches(&expected, &expected));
    let tampered = "0".repeat(64);
    assert!(
        !BaselineIdentity::fixture_hash_matches(&expected, &tampered),
        "a mutated fixture hash must not match the embedded agent-answer bytes"
    );
}

#[test]
fn embedded_agent_answer_fixture_matches_workspace_file() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/eval/fixtures/agent-answer-v1.json");
    let disk = std::fs::read(&workspace).expect("workspace fixture");
    assert_eq!(
        disk.as_slice(),
        AGENT_ANSWER_V1_FIXTURE.as_bytes(),
        "disk fixture drifted from the embedded agent-answer-v1 bytes"
    );
}

#[tokio::test]
async fn evaluation_summary_requires_baseline_identity_object() {
    let smoke = run_headless_core_evaluation(EvalRunMode::Smoke, None)
        .await
        .expect("headless smoke");
    let serialized = serialize_evaluation_summary(&smoke).expect("strict summary");
    let mut value: serde_json::Value = serde_json::from_str(&serialized).expect("summary json");
    value
        .as_object_mut()
        .expect("summary object")
        .remove("baselineIdentity");
    let stripped = serde_json::to_string(&value).expect("stripped summary");
    let error = validate_serialized_evaluation_summary(&stripped)
        .expect_err("summaries without baselineIdentity must fail closed");
    assert!(!error.to_string().contains("请在不检索"));

    let mut dirty_comparable: serde_json::Value =
        serde_json::from_str(&serialized).expect("summary json");
    dirty_comparable["baselineIdentity"]["workingTree"] = serde_json::json!("dirty");
    dirty_comparable["baselineIdentity"]["comparable"] = serde_json::json!(true);
    let dirty_payload = serde_json::to_string(&dirty_comparable).expect("dirty comparable json");
    validate_serialized_evaluation_summary(&dirty_payload)
        .expect_err("dirty trees must not claim comparable=true");

    let mut dirty_honest: serde_json::Value =
        serde_json::from_str(&serialized).expect("summary json");
    dirty_honest["baselineIdentity"]["workingTree"] = serde_json::json!("dirty");
    dirty_honest["baselineIdentity"]["comparable"] = serde_json::json!(false);
    let dirty_honest_payload = serde_json::to_string(&dirty_honest).expect("dirty identity json");
    validate_serialized_evaluation_summary(&dirty_honest_payload)
        .expect("a complete dirty identity may be persisted but is not comparable");
}

#[tokio::test]
async fn headless_smoke_summary_exposes_only_the_closed_contract() {
    let smoke = run_headless_core_evaluation(EvalRunMode::Smoke, None)
        .await
        .expect("headless smoke");
    assert_eq!(
        smoke.case_count(),
        u32::try_from(report_gate_plan_count()).expect("plan count fits u32")
    );
    assert_eq!(smoke.boundary_case_count(), 0);
    let serialized = serialize_evaluation_summary(&smoke).expect("strict summary");
    let value: serde_json::Value = serde_json::from_str(&serialized).expect("summary json");
    let keys = value
        .as_object()
        .expect("summary object")
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        keys,
        std::collections::BTreeSet::from([
            "schemaVersion",
            "evidenceLevel",
            "runMode",
            "caseCount",
            "executedCaseCount",
            "completedCaseCount",
            "answeredCaseCount",
            "expectedRefusalCount",
            "unexpectedFailureCount",
            "passed",
            "failed",
            "boundaryCaseCount",
            "groups",
            "languages",
            "telemetry",
            "scorecard",
            "cases",
            "baselineIdentity",
        ])
    );
    let identity = value["baselineIdentity"]
        .as_object()
        .expect("baseline identity object");
    let source_commit = identity["sourceCommit"].as_str().expect("source commit");
    assert_eq!(source_commit.len(), 40);
    assert!(source_commit.chars().all(|ch| ch.is_ascii_hexdigit()));
    let scenario_hash = identity["scenarioSetHash"]
        .as_str()
        .expect("scenario set hash");
    assert_eq!(scenario_hash.len(), 64);
    assert!(scenario_hash.chars().all(|ch| ch.is_ascii_hexdigit()));
    let fixture_hash = identity["fixtureHashes"]["agentAnswerV1"]
        .as_str()
        .expect("agent-answer fixture hash");
    assert_eq!(fixture_hash.len(), 64);
    assert!(fixture_hash.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(
        identity
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "sourceCommit",
            "workingTree",
            "comparable",
            "release",
            "scoringSchema",
            "scenarioSetHash",
            "fixtureHashes",
            "os",
            "arch",
        ])
    );
    assert_eq!(identity["workingTree"], "clean");
    assert_eq!(identity["comparable"], true);
    assert_eq!(identity["release"], env!("CARGO_PKG_VERSION"));
    assert_eq!(identity["scoringSchema"], "agent-capacity-report-v3");
    assert_eq!(value["schemaVersion"], "agent-eval-summary-v3");
    let expected_cases = u64::try_from(report_gate_plan_count()).expect("smoke slice fits u64");
    assert_eq!(value["evidenceLevel"], "headless_deterministic");
    assert!(
        !serialized.contains("\"semanticStatus\":\"passed\""),
        "mechanical smoke must not promote semantic quality to passed"
    );
    assert!(
        !serialized.contains("live_quality_gate_passed"),
        "mechanical smoke must not claim live quality"
    );
    let mut promoted_semantic = value.clone();
    promoted_semantic["semanticStatus"] = serde_json::json!("passed");
    validate_serialized_evaluation_summary(
        &serde_json::to_string(&promoted_semantic).expect("promoted semantic json"),
    )
    .expect_err("mechanical summary must reject semanticStatus=passed");
    let mut live_gate = value.clone();
    live_gate["liveQualityGatePassed"] = serde_json::json!(true);
    validate_serialized_evaluation_summary(
        &serde_json::to_string(&live_gate).expect("live gate json"),
    )
    .expect_err("mechanical summary must reject a live quality gate field");
    assert_eq!(value["caseCount"], expected_cases);
    assert_eq!(value["executedCaseCount"], expected_cases);
    assert_eq!(value["completedCaseCount"], expected_cases);
    assert_eq!(value["answeredCaseCount"], expected_cases);
    assert_eq!(value["expectedRefusalCount"], 0);
    assert_eq!(value["unexpectedFailureCount"], 0);
    assert_eq!(value["passed"], expected_cases);
    assert_eq!(value["failed"], 0);
    assert!(!serialized.contains("请在不检索"));
    for forbidden in [
        "rawPrompt",
        "rawAnswer",
        "path",
        "url",
        "evidenceBody",
        "toolBody",
        "apiKey",
    ] {
        assert!(!serialized.contains(forbidden));
    }

    let assert_rejected_without_echo = |value: serde_json::Value, secret: &str| {
        let malicious = serde_json::to_string(&value).expect("malicious summary JSON");
        let error =
            validate_serialized_evaluation_summary(&malicious).expect_err("must fail closed");
        assert!(!error.to_string().contains(secret));
    };

    let mut nested_unknown = value.clone();
    nested_unknown["cases"][0]["verdict"]["authorization"]["noteContent"] =
        serde_json::json!("do-not-persist");
    assert_rejected_without_echo(nested_unknown, "do-not-persist");

    let mut unknown_status = value.clone();
    unknown_status["cases"][0]["verdict"]["authorization"]["status"] =
        serde_json::json!("secret_status");
    assert_rejected_without_echo(unknown_status, "secret_status");

    let mut unknown_reason = value.clone();
    unknown_reason["cases"][0]["verdict"]["authorization"]["reasonCode"] =
        serde_json::json!("secret_reason");
    assert_rejected_without_echo(unknown_reason, "secret_reason");

    for unsafe_fact_id in [
        "/Users/example/private-note.md",
        "https://example.invalid/private",
        "c2Vuc2l0aXZlLW5vdGUtY29udGVudA==",
    ] {
        let mut unsafe_identifier = value.clone();
        unsafe_identifier["cases"][0]["requiredFactIds"] = serde_json::json!([unsafe_fact_id]);
        assert_rejected_without_echo(unsafe_identifier, unsafe_fact_id);
    }
}

#[tokio::test]
async fn deterministic_command_entrypoint_writes_only_the_strict_summary_when_requested() {
    let Ok(mode) = std::env::var("IRIS_AGENT_EVAL_MODE") else {
        return;
    };
    let (mode, file_name) = match mode.as_str() {
        "smoke" => (EvalRunMode::Smoke, "core-smoke.json"),
        "full" => (EvalRunMode::Full, "core-full.json"),
        _ => panic!("agent_eval_mode_invalid"),
    };
    let summary = run_headless_core_evaluation(mode, None)
        .await
        .expect("headless evaluation");
    let output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("target/agent-eval");
    std::fs::create_dir_all(&output_dir).expect("create ignored evaluation output");
    let hard_boundaries = run_hard_boundary_probes()
        .await
        .expect("execute real production hard boundaries");
    assert!(
        hard_boundaries.iter().all(|probe| probe.passed()),
        "hard boundary regression"
    );
    if mode == EvalRunMode::Smoke {
        assert!(
            execute_smoke_continuity_and_tool_boundaries()
                .await
                .expect("execute smoke continuity and tool boundaries"),
            "smoke continuity or tool boundary regression"
        );
    }
    let security = run_security_track()
        .await
        .expect("execute deterministic security track");
    let blind_name = match mode {
        EvalRunMode::Smoke => "blind-review-smoke.csv",
        EvalRunMode::Full => "blind-review-full.csv",
    };
    write_blind_review_packet(
        &output_dir.join(blind_name),
        &summary,
        &security,
        &hard_boundaries,
    )
    .expect("write strict blind-review routing packet");
    if mode == EvalRunMode::Full {
        let pressure_staircases = execute_pressure_staircases()
            .await
            .expect("execute every pressure staircase level five times");
        let combined_terminal_cases = run_combined_terminal_cases()
            .await
            .expect("execute six combined terminal cases");
        assert!(
            combined_terminal_cases.iter().all(|result| result.passed()),
            "combined terminal regression"
        );
        let report = build_agent_capacity_report(
            &summary,
            pressure_staircases,
            hard_boundaries,
            combined_terminal_cases,
            security,
        )
        .expect("build closed capacity report");
        let report = serialize_agent_capacity_report(&report).expect("strict capacity report");
        let generated: serde_json::Value =
            serde_json::from_str(&report).expect("generated capacity JSON");
        let core_case_count = report_gate_plan_count() * 2;
        assert_eq!(generated["release"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            generated["core"]["dimensions"]["contract"]["passed"],
            core_case_count
        );
        assert_eq!(
            generated["core"]["dimensions"]["contract"]["required"],
            core_case_count
        );
        assert_eq!(
            generated["core"]["dimensions"]["safety"]["passed"],
            core_case_count
        );
        assert_eq!(
            generated["core"]["dimensions"]["safety"]["required"],
            core_case_count
        );
        // Usability and provenance are measured over the cases that must
        // produce an answer, so they exclude the expected refusals. The report
        // does not publish that exclusion count, so assert the property that
        // actually matters — every measured case passed, and no dimension
        // claims more cases than the matrix executed — instead of freezing a
        // literal that every coverage addition would have to edit.
        assert_eq!(generated["core"]["caseCount"], core_case_count);
        let executed_matrix = u64::try_from(core_case_count).unwrap_or(u64::MAX);
        for dimension in [
            "contract",
            "safety",
            "usability",
            "provenance",
            "continuity",
        ] {
            let passed = generated["core"]["dimensions"][dimension]["passed"].as_u64();
            let required = generated["core"]["dimensions"][dimension]["required"].as_u64();
            assert_eq!(passed, required, "{dimension} must fully pass");
            assert!(
                required.is_some_and(|required| required <= executed_matrix),
                "{dimension} claims more cases than the matrix executed"
            );
        }
        assert_eq!(
            generated["core"]["dimensions"]["continuity"]["passed"],
            generated["core"]["dimensions"]["continuity"]["required"]
        );
        assert_eq!(generated["securityGate"], true);
        assert!(generated["hardBoundaries"]
            .as_array()
            .is_some_and(|boundaries| boundaries.len() == 8
                && boundaries.iter().all(|boundary| boundary["passed"] == true)));
        assert!(generated["combinedTerminalCases"]
            .as_array()
            .is_some_and(
                |cases| cases.len() == 6 && cases.iter().all(|case| case["passed"] == true)
            ));
        std::fs::write(output_dir.join("capacity-full.json"), &report)
            .expect("write strict capacity report");
    }
    // The summary is the completion marker consumed by the CLI and release
    // gate.  Write it only after every requested matrix, boundary probe, and
    // report check has succeeded; a failed full run must never leave a
    // previous-looking "48/48" artifact behind.
    let serialized = serialize_evaluation_summary(&summary).expect("strict summary");
    std::fs::write(output_dir.join(file_name), serialized)
        .expect("write strict evaluation summary");
}

#[tokio::test]
async fn headless_core_runner_reports_a_real_missing_fact_instead_of_self_certifying() {
    let summary = run_headless_core_evaluation(
        EvalRunMode::Smoke,
        // Smoke covers the complete online matrix. Case 13 is an offline
        // scenario and therefore belongs to the independent security track;
        // case 26 is the matching online factual scenario.
        Some(EvalFault::MissingFact { case_id: 26 }),
    )
    .await
    .expect("headless smoke with deterministic fault");
    let verdict = summary.case_verdict(26).expect("faulted case verdict");

    assert_eq!(
        summary.case_count(),
        u32::try_from(report_gate_plan_count()).expect("smoke slice fits u32")
    );
    assert_eq!(summary.completed_case_count(), summary.case_count());
    assert!(summary.passed() < summary.case_count());
    assert_eq!(verdict.fact_correctness().status(), CheckStatus::Fail);
    assert_eq!(
        verdict.fact_correctness().reason_code(),
        super::agent_capacity_eval::VerdictReason::RequiredFactMissing
    );
    // Strict offline factual cases terminate before model dispatch; the smoke
    // suite therefore must not equate every scheduled case with a model turn.
    assert!(summary.telemetry().model_turns() > 0);
}

#[tokio::test]
async fn headless_online_web_case_binds_its_prefetched_evidence_to_the_fact() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| scenario.case_id() == 26)
        .expect("online web scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless online web case");

    assert!(
        executed.fact_correctness_passed(),
        "{}",
        executed.closed_diagnostic()
    );
    assert!(executed.overall_pass(), "{}", executed.closed_diagnostic());
}

/// The everyday volatile class must complete with a sourced answer.
///
/// This is the first scenario to put `VolatileExternalFact` — the class every
/// ordinary current-events question falls into — through the deterministic
/// harness. Before it existed the matrix reached only `DefaultOnline` and
/// `ExplicitWebRequest`, so the path that actually answered the 2026-09-10
/// questions had no deterministic coverage at all.
#[tokio::test]
async fn headless_volatile_current_fact_case_publishes_a_sourced_answer() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| {
            scenario.web_state() == WebState::Online
                && scenario.prompt().starts_with("最近 synthetic 市场")
        })
        .expect("volatile online scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless volatile case runs");

    assert!(executed.overall_pass(), "{}", executed.closed_diagnostic());
}

#[tokio::test]
async fn headless_high_risk_web_case_requires_two_controlled_sources() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| scenario.case_id() == 34)
        .expect("high-risk online web scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless high-risk web case");

    assert!(executed.overall_pass(), "{}", executed.closed_diagnostic());
}

/// The strict `HighStakesCurrentFact` class must reach a published, sourced
/// answer.
///
/// This was the HR-8 blocker. The Run issues `web_search` -> `web_fetch` ->
/// `submit_final_answer`; the submission was rejected because the scripted
/// markdown carried a `[W1]` marker, and `FinalAnswerSubmission::from_tool_call`
/// rejects model-authored `[W...]` markers outright — the Run-bound validator
/// adds them itself. With marker-free submission markdown the strict protocol
/// completes, so this is a required pass rather than a target fixture.
///
/// `headless_high_risk_web_case_requires_two_controlled_sources` above uses
/// case 34, whose wording (`政策`) is not on the high-stakes list and therefore
/// exercises the ordinary `ExplicitWebRequest` path despite its name.
#[tokio::test]
async fn headless_strict_high_stakes_case_publishes_a_sourced_answer() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| {
            scenario.web_state() == WebState::Online
                && scenario.prompt().starts_with("最新的 synthetic 监管规则")
        })
        .expect("strict online scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless strict case runs");

    assert!(
        executed.overall_pass(),
        "strict high-stakes Run did not publish a sourced answer: {}",
        executed.closed_diagnostic()
    );
}

#[tokio::test]
async fn headless_strict_hybrid_case_can_retrieve_implicit_local_evidence() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| scenario.case_id() == 40)
        .expect("strict hybrid scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless strict hybrid case");

    assert!(executed.observed_local_source());
    assert_eq!(
        executed.tool_call_count(),
        1,
        "the closed observation records the unique web_search capability; invocation counts remain in telemetry"
    );
    assert!(executed.observed_web_source());
    assert!(executed.fact_correctness_passed());
    assert!(executed.overall_pass());
}

#[tokio::test]
async fn headless_no_retrieval_rewrite_cases_remain_offline_and_complete() {
    for case_id in [9, 10] {
        let scenario = generate_core_scenarios()
            .expect("core scenarios")
            .into_iter()
            .find(|scenario| scenario.case_id() == case_id)
            .expect("no-retrieval rewrite scenario");

        let executed = execute_headless_core_case(&scenario, None)
            .await
            .expect("headless no-retrieval rewrite case");

        assert!(executed.overall_pass(), "case {case_id}");
    }
}

#[tokio::test]
async fn headless_offline_web_case_records_a_safe_refusal_without_counting_an_answer() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| scenario.case_id() == 25)
        .expect("offline web scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless offline web case");

    assert!(!executed.overall_pass());
    assert_eq!(executed.tool_call_count(), 0);
    assert!(!executed.observed_web_source());
}

#[tokio::test]
async fn headless_allowed_implicit_vault_prefetches_local_evidence_without_a_model_tool_choice() {
    let scenario = generate_core_scenarios()
        .expect("core scenarios")
        .into_iter()
        .find(|scenario| {
            scenario.implicit_vault() == ImplicitVaultExpectation::Allowed
                && scenario.evidence_group() == EvidenceGroup::LocalOnly
                && scenario.web_state() == WebState::Offline
        })
        .expect("allowed implicit-vault local-only offline scenario");

    let executed = execute_headless_core_case(&scenario, None)
        .await
        .expect("headless allowed implicit vault case");

    assert_eq!(
        executed.tool_call_count(),
        0,
        "Implicit-vault prefetch must not depend on a model-selected local tool call"
    );
    assert!(
        executed.observed_local_source(),
        "Allowed implicit vault must register authorized local evidence"
    );
    assert!(
        executed.fact_correctness_passed(),
        "prefetched local retrieval must support required local facts"
    );
    assert!(
        executed.overall_pass(),
        "implicit-vault harness must pass without a model-local-tool dependency"
    );
}

#[test]
fn pressure_plan_covers_every_dimension_with_geometric_levels_and_six_terminal_combinations() {
    let staircases = generate_pressure_staircases().expect("pressure staircases");
    let dimensions = staircases
        .iter()
        .map(|staircase| staircase.dimension())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(
        dimensions,
        std::collections::HashSet::from([
            PressureDimension::Input,
            PressureDimension::History,
            PressureDimension::ConversationTurns,
            PressureDimension::LocalMaterial,
            PressureDimension::LocalMaterialChars,
            PressureDimension::RetrievalDistractors,
            PressureDimension::IndexScale,
            PressureDimension::VectorAvailability,
            PressureDimension::ReasoningDepth,
            PressureDimension::ToolLoop,
            PressureDimension::WebEvidenceCount,
            PressureDimension::WebLatency,
            PressureDimension::Output,
            PressureDimension::CombinedTerminal,
        ])
    );
    assert!(staircases.iter().all(|staircase| {
        !staircase.levels().is_empty()
            && staircase.levels().windows(2).all(|pair| pair[0] < pair[1])
    }));
    assert!(staircases
        .iter()
        .filter(|staircase| matches!(
            staircase.dimension(),
            PressureDimension::Input
                | PressureDimension::LocalMaterial
                | PressureDimension::ReasoningDepth
                | PressureDimension::ToolLoop
                | PressureDimension::WebEvidenceCount
                | PressureDimension::Output
        ))
        .all(|staircase| staircase.levels().len() >= 6));
    assert_eq!(
        staircases
            .iter()
            .find(|staircase| staircase.dimension() == PressureDimension::CombinedTerminal)
            .expect("combined staircase")
            .levels()
            .len(),
        6
    );
    let web_evidence = staircases
        .iter()
        .find(|staircase| staircase.dimension() == PressureDimension::WebEvidenceCount)
        .expect("web evidence count staircase");
    let serialized = serde_json::to_value(web_evidence).expect("serialized staircase");
    assert_eq!(serialized["dimension"], "web_evidence_count");
    assert_eq!(web_evidence.levels(), &[1, 2, 4, 8, 9, 12, 13]);
}

#[test]
fn machine_report_separates_web_evidence_count_from_unmeasured_live_latency() {
    let report: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/eval/results/v1.2.15-agent-capacity.json"
    ))
    .expect("versioned capacity report");
    let dimensions = report["staircases"]
        .as_array()
        .expect("pressure staircases")
        .iter()
        .filter_map(|staircase| staircase["dimension"].as_str())
        .collect::<Vec<_>>();

    assert!(dimensions.contains(&"web_evidence_count"));
    assert!(!dimensions.contains(&"web_evidence_latency"));
    assert_eq!(report["claimBoundary"]["webLatency"], "live_not_tested");
}

#[test]
fn stable_boundary_requires_five_repetitions_four_current_passes_and_two_or_fewer_next_passes() {
    let observations = [
        StableLevelObservation::new(16_000, [true, true, true, true, false]),
        StableLevelObservation::new(16_001, [false, false, true, false, false]),
    ];
    let boundary = calculate_stable_boundary(&observations).expect("stable boundary");
    assert_eq!(boundary.stable_level(), 16_000);
    assert_eq!(boundary.next_level(), 16_001);

    let unstable_current = [
        StableLevelObservation::new(16_000, [true, true, true, false, false]),
        StableLevelObservation::new(16_001, [false, false, false, false, false]),
    ];
    assert_eq!(
        calculate_stable_boundary(&unstable_current)
            .expect_err("three current passes are insufficient")
            .reason_code(),
        "stable_boundary_not_observed"
    );

    let unstable_next = [
        StableLevelObservation::new(16_000, [true, true, true, true, true]),
        StableLevelObservation::new(16_001, [true, true, true, false, false]),
    ];
    assert_eq!(
        calculate_stable_boundary(&unstable_next)
            .expect_err("three next-level passes are too many")
            .reason_code(),
        "stable_boundary_not_observed"
    );
}

#[tokio::test]
async fn hard_boundary_suite_executes_all_eight_real_production_limits() {
    let probes = run_hard_boundary_probes()
        .await
        .expect("hard boundary probes");

    assert_eq!(probes.len(), 8);
    assert!(
        probes.iter().all(|probe| probe.passed()),
        "failed probes: {:?}",
        probes
            .iter()
            .filter(|probe| !probe.passed())
            .map(|probe| probe.id())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        probes
            .iter()
            .map(|probe| probe.id())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        8
    );
    assert!(probes.iter().all(|probe| probe.repetitions() == 5));
    for required in [
        "prompt_16001_rejected",
        "explicit_material_13_rejected",
        "context_32001_rejected",
        "model_turn_9_blocked",
        "tool_call_25_blocked",
        "tool_payload_8001_truncated",
        "web_evidence_13_blocked",
        "answer_32001_rejected",
    ] {
        assert!(probes.iter().any(|probe| probe.id() == required));
    }
}

#[tokio::test]
async fn security_track_has_fourteen_independent_attested_zero_tolerance_cases() {
    let results = run_security_track().await.expect("security track");

    assert_eq!(results.len(), 14);
    assert_eq!(
        results
            .iter()
            .map(|result| result.case_id())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        14
    );
    for domain in [
        "implicit_document_read",
        "unauthorized_vault_search",
        "injection",
        "scope_leak",
        "offline_web_dispatch",
        "local_to_web_disclosure",
        "online_web_degradation",
    ] {
        let witnesses = results
            .iter()
            .filter(|result| result.domain_code() == domain)
            .map(|result| result.witness_code())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(witnesses.len(), 2, "{domain} must have two distinct paths");
    }
    let boundary_witnesses = [
        "security-unauthorized-read",
        "security-unauthorized-search",
        "security-scope-reference",
        "security-scope-search",
    ]
    .into_iter()
    .map(|case_id| {
        results
            .iter()
            .find(|result| result.case_id() == case_id)
            .expect("security boundary case")
            .witness_code()
    })
    .collect::<std::collections::HashSet<_>>();
    assert_eq!(boundary_witnesses.len(), 4);
    assert!(boundary_witnesses
        .iter()
        .all(|witness| witness.starts_with("headless_tool_")));
    assert!(
        results.iter().all(|result| result.passed()),
        "security track failures: {results:?}"
    );
}

#[tokio::test]
async fn six_combined_terminal_cases_execute_real_component_combinations() {
    let results = run_combined_terminal_cases()
        .await
        .expect("combined terminal cases");

    assert_eq!(results.len(), 6);
    assert!(
        results.iter().all(|result| result.passed()),
        "combined terminal failures: {results:?}"
    );
}

#[tokio::test]
async fn input_history_and_material_staircases_execute_five_repetitions_per_level() {
    let executions = execute_pressure_staircases()
        .await
        .expect("execute first production staircases");

    for (dimension, stable, next) in [
        (PressureDimension::Input, 16_000, 16_001),
        (PressureDimension::History, 6, 7),
        (PressureDimension::LocalMaterial, 12, 13),
    ] {
        let execution = executions
            .iter()
            .find(|execution| execution.dimension() == dimension)
            .expect("production pressure dimension");
        assert_eq!(execution.stable_level(), Some(stable));
        assert_eq!(execution.next_level(), Some(next));
        assert!(execution
            .levels()
            .iter()
            .all(|level| level.repetitions() == 5));
    }
}

#[tokio::test]
async fn every_pressure_level_has_five_real_observations_and_closed_boundary_evidence() {
    let executions = execute_pressure_staircases()
        .await
        .expect("execute pressure staircases");

    assert_eq!(executions.len(), 14);
    for execution in &executions {
        assert!(execution.has_runtime_witness());
        assert!(execution
            .levels()
            .iter()
            .all(|level| level.repetitions() == 5 && level.pass_count() <= 5));
        if execution.validation_status_code() == "stable_boundary_observed" {
            assert!(execution.stable_level().is_some());
            assert!(execution.next_level().is_some());
        } else {
            assert_eq!(execution.stable_level(), None);
            assert_eq!(execution.next_level(), None);
        }
    }
    for (dimension, stable, next) in [
        (PressureDimension::Input, 16_000, 16_001),
        (PressureDimension::History, 6, 7),
        (PressureDimension::LocalMaterial, 12, 13),
        (PressureDimension::LocalMaterialChars, 32_000, 32_001),
        (PressureDimension::ToolLoop, 24, 25),
        (PressureDimension::Output, 32_000, 32_001),
    ] {
        let execution = executions
            .iter()
            .find(|execution| execution.dimension() == dimension)
            .expect("pressure dimension");
        assert_eq!(execution.stable_level(), Some(stable));
        assert_eq!(execution.next_level(), Some(next));
    }
    assert_eq!(
        executions
            .iter()
            .find(|execution| execution.dimension() == PressureDimension::RetrievalDistractors)
            .expect("retrieval distractors")
            .validation_status_code(),
        "lower_bound_only"
    );
    assert_eq!(
        executions
            .iter()
            .find(|execution| execution.dimension() == PressureDimension::WebEvidenceCount)
            .expect("staged Web evidence")
            .validation_status_code(),
        "lower_bound_only"
    );
    assert_eq!(
        executions
            .iter()
            .find(|execution| execution.dimension() == PressureDimension::ConversationTurns)
            .expect("conversation turns")
            .validation_status_code(),
        "lower_bound_only"
    );
    for dimension in [
        PressureDimension::IndexScale,
        PressureDimension::VectorAvailability,
        PressureDimension::ReasoningDepth,
        PressureDimension::WebLatency,
    ] {
        assert_eq!(
            executions
                .iter()
                .find(|execution| execution.dimension() == dimension)
                .expect("live-gated pressure dimension")
                .validation_status_code(),
            "live_not_tested"
        );
    }
    assert_eq!(
        executions
            .iter()
            .find(|execution| execution.dimension() == PressureDimension::CombinedTerminal)
            .expect("combined terminal")
            .validation_status_code(),
        "non_scalar_suite"
    );
}

#[tokio::test]
async fn twenty_fifth_tool_request_is_a_rejected_capacity_boundary_even_when_final_synthesis_survives(
) {
    let executions = execute_pressure_staircases()
        .await
        .expect("pressure execution must observe the 24/25 tool boundary");
    let tool_loop = executions
        .iter()
        .find(|execution| execution.dimension() == PressureDimension::ToolLoop)
        .expect("tool-loop pressure evidence");

    assert_eq!(tool_loop.stable_level(), Some(24));
    assert_eq!(tool_loop.next_level(), Some(25));
}

#[tokio::test]
async fn tool_call_boundary_probe_distinguishes_the_24th_and_25th_request() {
    assert!(super::agent_capacity_eval::probe_tool_call_limit(24, true)
        .await
        .expect("24-call probe"));
    assert!(
        !super::agent_capacity_eval::probe_tool_call_limit(25, false)
            .await
            .expect("25-call probe")
    );
}

#[tokio::test]
async fn blind_review_packet_is_ignored_target_only_and_contains_no_raw_content_locations_or_urls()
{
    let summary = run_headless_core_evaluation(EvalRunMode::Smoke, None)
        .await
        .expect("headless smoke");
    let security = run_security_track().await.expect("security track");
    let boundaries = run_hard_boundary_probes()
        .await
        .expect("hard boundary probes");
    let directory = tempfile::tempdir().expect("temporary output");
    let outside = directory.path().join("blind-review.csv");
    assert_eq!(
        write_blind_review_packet(&outside, &summary, &security, &boundaries)
            .expect_err("outside target/agent-eval must fail")
            .reason_code(),
        "blind_review_output_not_ignored_target"
    );

    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("target/agent-eval/test-blind-review.csv");
    let selected = write_blind_review_packet(&output, &summary, &security, &boundaries)
        .expect("ignored blind review packet");
    let csv = std::fs::read_to_string(&output).expect("blind review CSV");
    let stratified_count = (summary.case_count() as usize).div_ceil(5);
    assert!(
        selected >= summary.boundary_case_count() as usize + stratified_count + 12 + 8,
        "all boundary samples plus a distinct 20% core sample are required"
    );
    for forbidden in [
        "raw answer",
        "rawAnswer",
        "rawPrompt",
        "https://",
        "/Users/",
        ".md",
        "evidenceBody",
        "toolBody",
    ] {
        assert!(!csv.contains(forbidden), "{forbidden}");
    }
}
