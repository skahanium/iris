use super::domain_executor::{
    AuthorityConflict, AuthorizedDomainMaterial, DomainAccessTrace, DomainExecutor,
    DomainExecutorKind, DomainMaterialRole, DomainVerificationError,
};
use super::run_contract::{
    CapabilityId, ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed,
    Modality, RiskClass, SecurityDomain,
};

fn envelope(context: ContextMode, material_needs: Vec<MaterialNeed>) -> ExecutionEnvelope {
    ExecutionEnvelope {
        effect: Effect::Draft,
        context,
        freshness: Freshness::Offline,
        effort: Effort::Direct,
        security_domain: SecurityDomain::Normal,
        risk: RiskClass::ReadOnly,
        modalities: vec![Modality::Text],
        material_needs,
        required_capabilities: vec![CapabilityId::new("model.text")],
        explicit_constraints: vec![],
    }
}

fn material(role: DomainMaterialRole, label: &str, content: &str) -> AuthorizedDomainMaterial {
    AuthorizedDomainMaterial {
        role,
        label: label.into(),
        content: content.into(),
    }
}

#[test]
fn golden_official_writing_separates_authority_content_from_exemplar_style() {
    let exemplar = material(
        DomainMaterialRole::Exemplar,
        "请示范文",
        "北京市教育局：现将2026年3月12日专项工作情况请示如下。",
    );
    let authority = material(
        DomainMaterialRole::Authority,
        "档案管理条例",
        "涉及档案调阅应当履行审批程序，不得擅自对外披露。",
    );
    let plan = DomainExecutor::plan(
        &envelope(
            ContextMode::ExplicitReferences,
            vec![MaterialNeed::Authority, MaterialNeed::Exemplar],
        ),
        "请结合制度写一份请示",
        &[exemplar, authority],
        &[],
    );

    assert_eq!(
        plan.active_executors,
        vec![
            DomainExecutorKind::WorkAnalysis,
            DomainExecutorKind::OfficialWriting
        ]
    );
    assert!(plan.prompt_instructions.contains("内容依据"));
    assert!(plan.prompt_instructions.contains("写法参考"));
    assert!(plan
        .prompt_instructions
        .contains("不得把 authority 当作语言模板"));
    assert!(plan
        .prompt_instructions
        .contains("不得把 exemplar 当作内容结论"));
    assert!(plan.rendered_context.contains("role=\"authority\""));
    assert!(plan.rendered_context.contains("role=\"exemplar\""));
    assert!(plan.style_blueprint.is_some());
}

#[test]
fn golden_exemplar_facts_are_rejected_when_not_supported_by_user_or_other_evidence() {
    let exemplar = material(
        DomainMaterialRole::Exemplar,
        "通知范文",
        "北京市教育局将于2026年3月12日组织专项检查。",
    );
    let plan = DomainExecutor::plan(
        &envelope(
            ContextMode::ExplicitReferences,
            vec![MaterialNeed::Exemplar],
        ),
        "起草一份检查通知",
        &[exemplar],
        &[],
    );

    assert!(plan.style_blueprint.as_ref().is_some_and(|blueprint| {
        !blueprint.instructions.contains("北京市教育局") && !blueprint.instructions.contains("2026")
    }));
    assert!(matches!(
        plan.verify_output("北京市教育局将于2026年3月12日组织专项检查。"),
        Err(DomainVerificationError::ExemplarFactLeak { .. })
    ));
    assert!(plan.verify_output("请各有关单位按程序报送材料。").is_ok());
}

#[test]
fn golden_work_analysis_cites_authority_without_using_exemplar_as_conclusion() {
    let plan = DomainExecutor::plan(
        &envelope(
            ContextMode::ExplicitReferences,
            vec![MaterialNeed::Authority, MaterialNeed::Exemplar],
        ),
        "分析档案调阅流程和风险",
        &[
            material(
                DomainMaterialRole::Authority,
                "档案管理条例",
                "档案调阅应经审批。",
            ),
            material(
                DomainMaterialRole::Exemplar,
                "旧请示",
                "建议立即全部开放档案。",
            ),
        ],
        &[],
    );

    assert!(plan.prompt_instructions.contains("已知事实 / 待确认事实"));
    assert!(plan.prompt_instructions.contains("相关规范依据"));
    assert!(plan.prompt_instructions.contains("可选方案"));
    assert!(plan
        .prompt_instructions
        .contains("authority 应给出来源标注"));
    assert!(plan
        .prompt_instructions
        .contains("exemplar 不得作为分析结论依据"));
}

#[test]
fn golden_unresolved_authority_conflict_requires_user_decision_without_inventing_rule() {
    let plan = DomainExecutor::plan(
        &envelope(
            ContextMode::ExplicitReferences,
            vec![MaterialNeed::Authority],
        ),
        "哪个审批时限适用？",
        &[
            material(DomainMaterialRole::Authority, "制度甲", "审批时限为五日。"),
            material(DomainMaterialRole::Authority, "制度乙", "审批时限为十日。"),
        ],
        &[AuthorityConflict {
            labels: vec!["制度甲".into(), "制度乙".into()],
            effectiveness_confirmed: false,
            relationship_summary: None,
        }],
    );

    assert!(plan.requires_user_decision);
    assert!(plan.prompt_instructions.contains("conflict group"));
    assert!(plan.prompt_instructions.contains("请用户裁决"));
    assert!(plan
        .prompt_instructions
        .contains("不得合并或编造第三种规则"));
    assert!(plan.resume_state.awaiting_user_decision);
}

#[test]
fn golden_novel_without_explicit_reference_has_no_vault_read_or_search_trace() {
    let plan = DomainExecutor::plan(
        &envelope(ContextMode::Conversation, vec![]),
        "续写一段小说中的追逐场景",
        &[
            material(
                DomainMaterialRole::Authority,
                "不应读取",
                "人物设定：林某。",
            ),
            material(DomainMaterialRole::Exemplar, "不应读取", "历史章节。"),
        ],
        &[],
    );

    assert_eq!(
        plan.active_executors,
        vec![DomainExecutorKind::NovelBoundary]
    );
    assert!(plan.rendered_context.is_empty());
    assert!(plan.requested_capabilities.is_empty());
    assert_eq!(plan.access_trace, vec![DomainAccessTrace::ConversationOnly]);
    assert!(plan.prompt_instructions.contains("不得读取当前活动文档"));
    assert!(plan.prompt_instructions.contains("不得自动检索"));
}

#[test]
fn golden_novel_reads_only_two_explicit_references_in_declared_scope() {
    let plan = DomainExecutor::plan(
        &envelope(
            ContextMode::ExplicitReferences,
            vec![MaterialNeed::Reference],
        ),
        "根据 @角色设定 和 @上一章续写小说",
        &[
            material(DomainMaterialRole::Reference, "角色设定", "主角名为安宁。"),
            material(DomainMaterialRole::Reference, "上一章", "雨夜抵达车站。"),
            material(
                DomainMaterialRole::Authority,
                "禁止混入",
                "不应出现在小说上下文。",
            ),
        ],
        &[],
    );

    assert!(plan.rendered_context.contains("角色设定"));
    assert!(plan.rendered_context.contains("上一章"));
    assert!(!plan.rendered_context.contains("禁止混入"));
    assert_eq!(
        plan.access_trace,
        vec![
            DomainAccessTrace::ExplicitReference("角色设定".into()),
            DomainAccessTrace::ExplicitReference("上一章".into()),
        ]
    );
}
