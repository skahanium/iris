//! Base-question plan table and core-scenario generation.
//!
//! Split out of `agent_capacity_eval.rs` to keep that file inside the module
//! size budget. The parent re-exports these items, so callers keep importing
//! them from `agent_capacity_eval`.

use super::*;

pub(crate) const UNEXPECTED_EVAL_TOOL: &str = "unexpected_tool";

/// Closed language classes used by the core capacity suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScenarioLanguage {
    Chinese,
    English,
    Mixed,
}

/// One generated core scenario. The prompt itself remains an ephemeral fixture
/// concern; this contract carries only closed classes and bounded synthetic IDs.
#[derive(Debug, Clone)]
pub(crate) struct CoreScenario {
    pub(crate) base_question_id: u32,
    pub(crate) language: ScenarioLanguage,
    pub(crate) hard_boundary: bool,
    pub(crate) prompt: &'static str,
    pub(crate) manifest: CaseManifest,
}

impl CoreScenario {
    pub(crate) fn case_id(&self) -> u32 {
        parse_case_ordinal(&self.manifest.id)
            .expect("generated scenario IDs are validated")
            .0
    }

    pub(crate) const fn base_question_id(&self) -> u32 {
        self.base_question_id
    }

    pub(crate) const fn evidence_group(&self) -> EvidenceGroup {
        self.manifest.evidence_group
    }

    pub(crate) const fn web_state(&self) -> WebState {
        self.manifest.web_state
    }

    pub(crate) const fn language(&self) -> ScenarioLanguage {
        self.language
    }

    pub(crate) const fn prompt(&self) -> &'static str {
        self.prompt
    }

    pub(crate) const fn is_hard_boundary(&self) -> bool {
        self.hard_boundary
    }

    pub(crate) const fn implicit_vault(&self) -> ImplicitVaultExpectation {
        self.manifest.local_authorization.implicit_vault
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BaseQuestionPlan {
    pub(crate) group: EvidenceGroup,
    pub(crate) language: ScenarioLanguage,
    pub(crate) domain: &'static str,
    pub(crate) answer_mode: AnswerMode,
    pub(crate) prompt: &'static str,
}

impl BaseQuestionPlan {
    /// The declared evidence group. Exposed so the matrix test can assert that
    /// the generated scenarios match this table instead of a frozen count.
    pub(crate) const fn group(&self) -> EvidenceGroup {
        self.group
    }

    pub(crate) const fn language(&self) -> ScenarioLanguage {
        self.language
    }
}

pub(crate) const BASE_QUESTION_PLANS: [BaseQuestionPlan; 26] = [
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "writing",
        answer_mode: AnswerMode::Creative,
        prompt: "请在不检索任何资料的前提下，写一个三句式的产品发布开场白。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "rewrite",
        answer_mode: AnswerMode::Rewrite,
        prompt: "请把“我们需要尽快解决这个问题”改写得更具体、克制，不增加新事实。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "reasoning",
        answer_mode: AnswerMode::Creative,
        prompt: "请解释为什么反例足以否定全称命题，并给出一个纯虚构例子。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "planning",
        answer_mode: AnswerMode::Creative,
        prompt: "请设计一个不依赖外部资料的十五分钟复盘流程，限定为四步。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::English,
        domain: "writing",
        answer_mode: AnswerMode::Rewrite,
        prompt: "Rewrite this supplied synthetic status update, \"Build is green.\", in a concise, neutral tone without adding facts.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Mixed,
        domain: "engineering",
        answer_mode: AnswerMode::Creative,
        prompt: "用中文解释 idempotency，并用 one short English example 收尾；不要检索。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "notes",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "仅根据明确附带的 synthetic 项目笔记，列出已决定事项并逐条引用。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "project",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "根据授权的本地项目资料总结里程碑；联网开关不改变所需证据范围。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "从授权本地材料提炼三个研究假设，不得读取未授权笔记。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "meeting",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "根据本地会议记录生成行动项、负责人代号与依据引用。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::English,
        domain: "notes",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Summarize the explicitly authorized synthetic note and cite each claim.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::English,
        domain: "project",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Compare milestones across the authorized local project scope without using Web facts.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "current-events",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实 synthetic 产品今天的公开状态，并为所有时效性事实提供网页证据。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "market",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请查找并核实 synthetic 市场的最新公开规模估计，区分事实与不确定性。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "standards",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实 synthetic 标准的当前版本与发布日期，给出来源。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "software",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实 synthetic 软件当前稳定版本，不使用本地笔记作为版本事实。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "policy",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实并检索 synthetic 政策的最新公开文本，并说明无法验证时的限制。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::English,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Please verify the current public status of the synthetic study and cite supporting Web evidence.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "competitive-analysis",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实：把授权本地方案与 synthetic 竞品的最新公开信息对比，分别引用本地与网页证据。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "project-risk",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实：结合本地风险登记与最新公开依赖状态，给出证据分层的风险判断。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "technical-review",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实：用授权设计记录解释内部约束，再对照外部 synthetic API 的当前兼容性。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "decision-support",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实：根据本地决策标准和最新公开事实比较两个 synthetic 选项。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::English,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Please verify: compare the authorized local hypothesis with current public synthetic evidence and cite both.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Mixed,
        domain: "engineering",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "请核实：依据本地 design note 与最新 Web status 做 gap analysis，并清楚区分两类来源。",
    },
    // Coverage plans. The classifier can freeze a Run into more than one
    // verification class, and a gate that never reaches a class cannot report
    // anything about it. These two plans exist so the deterministic matrix
    // exercises the everyday volatile path and the strict high-stakes path,
    // not only the explicitly requested Web path.
    //
    // They are appended at the END of the table on purpose. Case ordinals are
    // derived from position, and shared helpers select scenarios by ordinal, so
    // inserting in the middle would silently repoint those selectors at other
    // questions. `core_case_identity_is_pinned` guards that mapping.
    //
    // Both avoid the revision-date guard phrases (`发布日期`, `修订日期`, …) on
    // purpose: those phrases deliberately downgrade a regulated-topic question
    // to the ordinary current-fact path, and that downgrade needs its own test
    // rather than being covered by accident here.
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "market",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "最近 synthetic 市场有哪些值得关注的公开变化？",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "regulatory",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "最新的 synthetic 监管规则适用于哪些情形？请给出适用依据。",
    },
];

/// Number of base questions the deterministic report gates execute.
///
/// Every declared plan is executed: a plan that cannot be driven end to end
/// belongs in a target fixture, never silently parked outside the report.
pub(crate) fn report_gate_plan_count() -> usize {
    BASE_QUESTION_PLANS.len()
}

/// Report-gate base-question count inside one evidence group.
pub(crate) fn report_gate_plan_count_for_group(group: EvidenceGroup) -> usize {
    BASE_QUESTION_PLANS
        .iter()
        .filter(|plan| plan.group == group)
        .count()
}

/// Generate the core matrix from the declared base questions. Each base
/// question keeps its language and evidence class across one Offline and one
/// Online variant; enabling Web therefore never changes the evidence contract.
pub(crate) fn generate_core_scenarios() -> Result<Vec<CoreScenario>, EvalContractError> {
    let mut scenarios = Vec::with_capacity(BASE_QUESTION_PLANS.len() * 2);
    let mut group_base_index = HashMap::<EvidenceGroup, usize>::new();
    for (base_index, plan) in BASE_QUESTION_PLANS.iter().copied().enumerate() {
        let ordinal_in_group = *group_base_index.entry(plan.group).or_insert(0);
        *group_base_index.entry(plan.group).or_insert(0) += 1;
        for web_state in [WebState::Offline, WebState::Online] {
            let case_ordinal = u32::try_from(scenarios.len() + 1)
                .map_err(|_| EvalContractError::new("core_case_count_invalid"))?;
            let base_question_id = u32::try_from(base_index + 1)
                .map_err(|_| EvalContractError::new("core_base_count_invalid"))?;
            let manifest = build_core_manifest(case_ordinal, plan, web_state, ordinal_in_group);
            manifest.validate()?;
            scenarios.push(CoreScenario {
                base_question_id,
                language: plan.language,
                hard_boundary: ordinal_in_group == 0 && web_state == WebState::Offline,
                prompt: plan.prompt,
                manifest,
            });
        }
    }
    validate_core_matrix(&scenarios)?;
    Ok(scenarios)
}

fn build_core_manifest(
    case_ordinal: u32,
    plan: BaseQuestionPlan,
    web_state: WebState,
    ordinal_in_group: usize,
) -> CaseManifest {
    let local_id = format!("local-{case_ordinal}");
    let web_id = format!("web-{case_ordinal}");
    let local_fact_id = format!("fact-local-{case_ordinal}");
    let web_fact_id = format!("fact-web-{case_ordinal}");
    let needs_local = matches!(plan.group, EvidenceGroup::LocalOnly | EvidenceGroup::Hybrid);
    let needs_web = matches!(plan.group, EvidenceGroup::WebOnly | EvidenceGroup::Hybrid);
    let implicit_vault = if needs_local && ordinal_in_group % 2 == 1 {
        ImplicitVaultExpectation::Allowed
    } else {
        ImplicitVaultExpectation::Forbidden
    };
    let explicit_reference_ids =
        if needs_local && implicit_vault == ImplicitVaultExpectation::Forbidden {
            vec![local_id.clone()]
        } else {
            Vec::new()
        };
    let mut available_sources = Vec::new();
    let mut required_sources = Vec::new();
    let mut required_facts = Vec::new();
    if needs_local {
        let source = RequiredSource {
            id: local_id.clone(),
            kind: SourceKind::Local,
        };
        available_sources.push(source.clone());
        required_sources.push(source);
        required_facts.push(RequiredFact {
            id: local_fact_id,
            allowed_sources: vec![local_id],
            citation_required: true,
        });
    }
    if needs_web {
        let source = RequiredSource {
            id: web_id.clone(),
            kind: SourceKind::Web,
        };
        available_sources.push(source.clone());
        required_sources.push(source);
        required_facts.push(RequiredFact {
            id: web_fact_id,
            allowed_sources: vec![web_id],
            citation_required: true,
        });
    }

    let local_tools = evaluation_local_read_tool_names();
    let mut allowed = Vec::new();
    let mut forbidden = Vec::new();
    for tool in local_tools {
        if needs_local {
            allowed.push(tool);
        } else {
            forbidden.push(tool);
        }
    }
    // In Online mode a model may decide to search even when Web evidence is
    // unnecessary. The evaluator records that as route inefficiency, not a
    // permission failure, unless the answer becomes contaminated.
    allowed.push("web_search".to_string());
    // Every ToolLoop Run has the immutable runtime.read capability. Runtime
    // reads are safe operational helpers, never evidence, and therefore use
    // one closed policy label rather than leaking individual tool names.
    allowed.push("runtime_context".to_string());

    CaseManifest {
        schema_version: "agent-answer-v1".to_string(),
        id: format!("case-{case_ordinal}"),
        evidence_group: plan.group,
        language: match plan.language {
            ScenarioLanguage::Chinese => "zh",
            ScenarioLanguage::English => "en",
            ScenarioLanguage::Mixed => "mixed",
        }
        .to_string(),
        domain: plan.domain.to_string(),
        web_state,
        local_authorization: LocalAuthorization {
            explicit_reference_ids,
            explicit_scope_id: None,
            explicit_scope_source_ids: Vec::new(),
            implicit_vault,
        },
        available_sources,
        required_facts,
        required_sources,
        tool_policy: ToolPolicy {
            allowed,
            forbidden,
            web_search: if needs_web {
                WebSearchPolicy::Required
            } else {
                WebSearchPolicy::Optional
            },
        },
        answer_mode: plan.answer_mode,
        citation_expectation: if needs_local || needs_web {
            CitationExpectation::Required
        } else {
            CitationExpectation::None
        },
        disclosure_constraints: if needs_web && web_state == WebState::Offline {
            vec!["web-offline-uncertainty".to_string()]
        } else {
            Vec::new()
        },
    }
}

/// The smallest core matrix the gate accepts. Coverage may grow; it must not
/// shrink, and it must not be rebalanced away from a verification class that a
/// production Run can actually be frozen into.
pub(crate) const CORE_MATRIX_MIN_CASES: usize = 48;

fn validate_core_matrix(scenarios: &[CoreScenario]) -> Result<(), EvalContractError> {
    // Derived from the declared plan table instead of a literal so that adding
    // a base question cannot desync the matrix from its own declaration.
    if scenarios.len() != BASE_QUESTION_PLANS.len() * 2 || scenarios.len() < CORE_MATRIX_MIN_CASES {
        return Err(EvalContractError::new("core_case_count_invalid"));
    }
    for group in [
        EvidenceGroup::NoRetrieval,
        EvidenceGroup::LocalOnly,
        EvidenceGroup::WebOnly,
        EvidenceGroup::Hybrid,
    ] {
        let expected = BASE_QUESTION_PLANS
            .iter()
            .filter(|plan| plan.group == group)
            .count()
            * 2;
        if expected == 0
            || scenarios
                .iter()
                .filter(|scenario| scenario.evidence_group() == group)
                .count()
                != expected
        {
            return Err(EvalContractError::new("core_group_distribution_invalid"));
        }
    }
    let language_count = |language| {
        scenarios
            .iter()
            .filter(|scenario| scenario.language() == language)
            .count()
    };
    // An Offline/Online pair shares one base question and language, hence every
    // count is even. The 70/20/10 language target is a proportion rather than a
    // frozen triple: a literal would have to be re-edited on every addition,
    // which is how a coverage table drifts out of date.
    let total = scenarios.len();
    for (language, target_percent) in [
        (ScenarioLanguage::Chinese, 70_u32),
        (ScenarioLanguage::English, 20),
        (ScenarioLanguage::Mixed, 10),
    ] {
        let count = language_count(language);
        if count % 2 != 0 {
            return Err(EvalContractError::new("core_language_distribution_invalid"));
        }
        let share = u32::try_from(count * 100 / total).unwrap_or(u32::MAX);
        if share.abs_diff(target_percent) > 5 {
            return Err(EvalContractError::new("core_language_distribution_invalid"));
        }
    }
    Ok(())
}
