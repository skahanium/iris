use std::collections::BTreeSet;

use super::final_answer_submission::{FinalAnswerBlock, FinalAnswerSubmission};
use super::provenance::{
    validate_final_answer_submission, InformationOrigin, ProvenancePolicy,
    ProvenanceValidationError,
};

fn policy() -> ProvenancePolicy {
    ProvenancePolicy {
        current_user_available: true,
        conversation_history_available: false,
        runtime_fact_available: false,
        authorized_material_count: 1,
        current_run_local_evidence_ids: BTreeSet::from([11]),
        current_run_web_evidence_ids: BTreeSet::from([21]),
        current_run_external_evidence_ids: BTreeSet::from([31]),
        strict_current_evidence: false,
    }
}

#[test]
fn structured_submission_renders_and_summarizes_mixed_origins() {
    let output = FinalAnswerSubmission {
        blocks: vec![
            FinalAnswerBlock {
                markdown: "你说预算是 10 万。".into(),
                sources: vec!["U".into()],
            },
            FinalAnswerBlock {
                markdown: "笔记显示现有供应商合同到期。".into(),
                sources: vec!["M1".into(), "L11".into()],
            },
            FinalAnswerBlock {
                markdown: "网页资料显示市场规模增长。".into(),
                sources: vec!["W21".into()],
            },
            FinalAnswerBlock {
                markdown: "这是我的建议。".into(),
                sources: vec!["I".into()],
            },
        ],
    };

    let validated =
        validate_final_answer_submission(&output, &policy()).expect("valid attribution");

    assert_eq!(validated.visible_content, "你说预算是 10 万。\n\n笔记显示现有供应商合同到期。\n\n网页资料显示市场规模增长。 [W21]\n\n这是我的建议。");
    assert_eq!(
        validated
            .source_summary
            .count(InformationOrigin::CurrentUserRequest),
        1
    );
    assert_eq!(
        validated
            .source_summary
            .count(InformationOrigin::UserAuthorizedMaterial),
        1
    );
    assert_eq!(
        validated
            .source_summary
            .count(InformationOrigin::LocalToolEvidence),
        1
    );
    assert_eq!(
        validated
            .source_summary
            .count(InformationOrigin::WebToolEvidence),
        1
    );
    assert_eq!(
        validated
            .source_summary
            .count(InformationOrigin::ModelInference),
        1
    );
}

#[test]
fn tool_or_inference_cannot_be_presented_as_user_input() {
    let output = FinalAnswerSubmission {
        blocks: vec![FinalAnswerBlock {
            markdown: "你提供的信息表明市场规模增长。".into(),
            sources: vec!["W21".into(), "I".into()],
        }],
    };

    assert_eq!(
        validate_final_answer_submission(&output, &policy()).unwrap_err(),
        ProvenanceValidationError::UserAttributionRequiresCurrentUserInput
    );
}

#[test]
fn historical_user_wording_cannot_be_backfilled_by_a_history_reference() {
    let mut historical_policy = policy();
    historical_policy.conversation_history_available = true;
    for markdown in [
        "你之前提到的预算是 10 万。",
        "根据你在前文提到的信息，预算是 10 万。",
        "In your earlier message, you mentioned a budget of 100,000.",
    ] {
        let output = FinalAnswerSubmission {
            blocks: vec![FinalAnswerBlock {
                markdown: markdown.to_string(),
                sources: vec!["H".to_string()],
            }],
        };
        assert_eq!(
            validate_final_answer_submission(&output, &historical_policy).unwrap_err(),
            ProvenanceValidationError::UserAttributionRequiresCurrentUserInput,
            "historical wording must not turn conversation history into current user input: {markdown}"
        );
    }
}

#[test]
fn strict_web_requires_each_substantive_block_to_bind_current_run_web_evidence() {
    let mut strict_policy = policy();
    strict_policy.strict_current_evidence = true;
    strict_policy.conversation_history_available = true;
    let output = FinalAnswerSubmission {
        blocks: vec![
            FinalAnswerBlock {
                markdown: "事实一。".into(),
                sources: vec!["W21".into()],
            },
            FinalAnswerBlock {
                markdown: "事实二。".into(),
                sources: vec!["H".into()],
            },
        ],
    };

    assert_eq!(
        validate_final_answer_submission(&output, &strict_policy).unwrap_err(),
        ProvenanceValidationError::StrictCurrentEvidenceMissing { block: 2 }
    );
}

#[test]
fn strict_web_allows_source_free_structural_heading_before_a_bound_fact_block() {
    let mut strict_policy = policy();
    strict_policy.strict_current_evidence = true;
    let output = FinalAnswerSubmission {
        blocks: vec![
            FinalAnswerBlock {
                markdown: "## 结论".into(),
                sources: Vec::new(),
            },
            FinalAnswerBlock {
                markdown: "HTTP 404 表示服务器找不到所请求的资源。".into(),
                sources: vec!["W21".into()],
            },
        ],
    };

    let validated = validate_final_answer_submission(&output, &strict_policy)
        .expect("structural Markdown is not an unsupported factual assertion");

    assert_eq!(
        validated.visible_content,
        "## 结论\n\nHTTP 404 表示服务器找不到所请求的资源。 [W21]"
    );
    assert_eq!(validated.attribution[0].sources, Vec::<String>::new());
}

#[test]
fn strict_current_fact_accepts_run_local_structured_external_evidence() {
    let mut strict_policy = policy();
    strict_policy.strict_current_evidence = true;
    let output = FinalAnswerSubmission {
        blocks: vec![FinalAnswerBlock {
            markdown: "结构化当前事实已核实。".into(),
            sources: vec!["E31".into()],
        }],
    };

    validate_final_answer_submission(&output, &strict_policy)
        .expect("a current-Run structured provider record is verified evidence");
}

#[test]
fn bare_ledger_ids_are_never_valid_source_references() {
    let output = FinalAnswerSubmission {
        blocks: vec![FinalAnswerBlock {
            markdown: "结构化当前事实已核实。".into(),
            sources: vec!["31".into()],
        }],
    };

    assert_eq!(
        validate_final_answer_submission(&output, &policy()).unwrap_err(),
        ProvenanceValidationError::UnknownOrUnauthorizedReference("31".to_string())
    );
}

#[test]
fn unknown_cross_run_or_mismatched_evidence_is_rejected() {
    let output = FinalAnswerSubmission {
        blocks: vec![FinalAnswerBlock {
            markdown: "网页资料表明增长。".into(),
            sources: vec!["W99".into()],
        }],
    };

    assert_eq!(
        validate_final_answer_submission(&output, &policy()).unwrap_err(),
        ProvenanceValidationError::UnknownOrUnauthorizedReference("W99".to_string())
    );
}

#[test]
fn malformed_unicode_reference_is_rejected_without_panicking() {
    let output = FinalAnswerSubmission {
        blocks: vec![FinalAnswerBlock {
            markdown: "网页资料表明增长。".into(),
            sources: vec!["你1".into()],
        }],
    };

    let outcome = std::panic::catch_unwind(|| validate_final_answer_submission(&output, &policy()));

    assert_eq!(
        outcome
            .expect("untrusted source references must not panic")
            .unwrap_err(),
        ProvenanceValidationError::UnknownOrUnauthorizedReference("你1".to_string())
    );
}
