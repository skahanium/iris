//! Model attempt accounting and shared tool-budget projection.
use super::*;

/// Retain only bounded Host check states, never candidate text or arbitrary
/// provider strings, for a rejected batch that cannot finish correction.
pub(super) fn confirmation_rejection_summary(results: &[(String, ToolCallResult)]) -> String {
    let mut checks = Vec::new();
    for (field, label) in [
        ("bodyText", "正文内容"),
        ("blockOrder", "段落顺序"),
        ("linkTargets", "链接目标"),
    ] {
        let states = results
            .iter()
            .filter_map(|(_, result)| result.output[field].as_str());
        let states = states.collect::<Vec<_>>();
        if states.contains(&"failed") {
            checks.push(format!("{label}检查未通过"));
        } else if states.contains(&"unknown") {
            checks.push(format!("{label}尚无法核实"));
        }
    }
    if checks.is_empty() {
        checks.push("候选预检未通过".to_string());
    }
    format!(
        "本批候选未进入确认，未写入。{}。纠偏或最终交付未能在本轮预算及校验限制内完成。",
        checks.join("；")
    )
}

/// Invoke every provider through the same quota boundary. Production adapters
/// account at their actual HTTP dispatch; deterministic providers use this seam.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn answer_budgeted_turn(
    provider: &(impl ToolLoopProvider + ?Sized),
    run_id: &str,
    messages: &[LlmMessage],
    tools: &[ToolSpec],
    budget: AgentModelTurnBudget,
    observer: &mut dyn StreamEventObserver,
    purpose: model_turn_ledger::AttemptPurpose,
) -> AppResult<GatewayResponse> {
    model_turn_ledger::with_purpose(purpose, async {
        if provider.manages_attempt_budget() {
            return provider
                .answer_turn(run_id, messages, tools, budget, observer)
                .await;
        }
        let lease =
            model_turn_ledger::claim_attempt(provider.budget_database(), run_id, purpose, budget)?;
        model_turn_ledger::mark_dispatched(provider.budget_database(), &lease)?;
        let response = provider
            .answer_turn(run_id, messages, tools, lease.budget, observer)
            .await;
        model_turn_ledger::settle_attempt(
            provider.budget_database(),
            &lease,
            response.as_ref().ok().map(|response| &response.usage),
        )?;
        if let Ok(response) = &response {
            let (prompt, completion, _) = resolved_turn_usage(response, messages, tools);
            if lease
                .budget
                .max_prompt_tokens
                .is_some_and(|limit| prompt > limit)
            {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            if lease
                .budget
                .max_turn_output_tokens
                .is_some_and(|limit| completion > limit)
            {
                return Err(AppError::run(if response.tool_calls.is_empty() {
                    SafeRunErrorCode::OutputTooLong
                } else {
                    SafeRunErrorCode::ToolLoopLimit
                }));
            }
        }
        response
    })
    .await
}

impl AgentToolLoop {
    pub(super) fn tool_call_limit(&self, class: ToolBudgetClass) -> u32 {
        match class {
            ToolBudgetClass::Local => self.max_local_tool_calls,
            ToolBudgetClass::Network => self.max_network_tool_calls,
            ToolBudgetClass::ExternalRead => self.max_external_read_tool_calls,
            ToolBudgetClass::Runtime => self.max_runtime_tool_calls,
            ToolBudgetClass::ConfirmedChange => self.max_confirmed_change_calls,
        }
    }

    pub(super) fn loop_projection(
        &self,
        remaining_model_turns: u32,
        tool_calls: u32,
        tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
        tools: &[ToolSpec],
        executor: &impl ToolLoopExecutor,
    ) -> LoopProjection {
        project_loop(
            remaining_model_turns,
            tool_calls,
            self.max_tool_calls,
            tool_calls_by_class,
            |class| self.tool_call_limit(class),
            &tools
                .iter()
                .filter_map(|tool| executor.tool_budget_class(&tool.name))
                .collect::<HashSet<_>>(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_tool_proposals<'a>(
    calls: &'a [ToolCall],
    active_allowed_tools: &HashSet<&str>,
    active_tools: &[ToolSpec],
    discovery_calls_this_turn: u32,
    tool_calls: u32,
    tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
    executions: &HashMap<String, ExecutionRecord>,
    fingerprints: &HashMap<String, u32>,
    loop_policy: &AgentToolLoop,
    executor: &impl ToolLoopExecutor,
) -> Vec<(&'a ToolCall, ToolCallDisposition)> {
    let mut planned_discovery = discovery_calls_this_turn;
    let mut planned_total = tool_calls;
    let mut planned_by_class = tool_calls_by_class.clone();
    let mut planned_fingerprints = fingerprints.clone();
    calls
        .iter()
        .map(|call| {
            let executor_owns_invalid_arguments =
                call.function.name == "spawn_subagent" && valid_call_identity(call);
            let rejection = if !active_allowed_tools.contains(call.function.name.as_str()) {
                Some("tool_not_in_run_surface")
            } else if call.id.trim().is_empty() {
                Some("missing_call_id")
            } else if !valid_call_arguments(call) && !executor_owns_invalid_arguments {
                Some("invalid_arguments_json")
            } else if !executor_owns_invalid_arguments
                && active_tools
                    .iter()
                    .find(|tool| tool.name == call.function.name)
                    .is_some_and(|tool| {
                        let args =
                            serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                                .unwrap_or_default();
                        matches!(
                            crate::ai_runtime::guardrails::verify_tool_args(
                                &call.function.name,
                                &args,
                                &tool.input_schema
                            ),
                            crate::ai_runtime::guardrails::GuardResult::Block { .. }
                        )
                    })
            {
                Some("arguments_schema_mismatch")
            } else if planned_total >= loop_policy.max_tool_calls {
                Some("tool_call_budget_exhausted")
            } else {
                None
            };
            if let Some(reason) = rejection {
                return (call, ToolCallDisposition::Rejected(reason));
            }
            let fingerprint = tool_fingerprint(call);
            if executions
                .get(&fingerprint)
                .is_some_and(ExecutionRecord::blocks_execution)
            {
                return (
                    call,
                    ToolCallDisposition::Rejected("tool_call_already_succeeded"),
                );
            }
            if is_discovery_call(call) && planned_discovery >= MAX_DISCOVERY_CALLS_PER_MODEL_TURN {
                return (call, ToolCallDisposition::Deferred);
            }
            let count = planned_fingerprints.entry(fingerprint).or_insert(0);
            *count = count.saturating_add(1);
            if *count > MAX_REPEAT_CALLS
                && !executions
                    .get(&tool_fingerprint(call))
                    .is_some_and(|record| record.compacted)
            {
                return (call, ToolCallDisposition::Rejected("tool_call_repeated"));
            }
            let Some(class) = executor.tool_budget_class(&call.function.name) else {
                return (
                    call,
                    ToolCallDisposition::Rejected("tool_not_in_run_surface"),
                );
            };
            let used = planned_by_class.entry(class).or_default();
            if *used >= loop_policy.tool_call_limit(class) {
                return (
                    call,
                    ToolCallDisposition::Rejected("tool_category_budget_exhausted"),
                );
            }
            if is_discovery_call(call) {
                planned_discovery = planned_discovery.saturating_add(1);
            }
            planned_total = planned_total.saturating_add(1);
            *used = used.saturating_add(1);
            (call, ToolCallDisposition::Dispatched)
        })
        .collect()
}

#[cfg(test)]
mod review_budget_projection_tests {
    use super::*;

    struct DynamicExecutor;
    impl ToolLoopExecutor for DynamicExecutor {
        fn mapped_tool_name(&self, name: &str) -> bool {
            name == "mcp_review_read"
        }
        fn execute<'a>(
            &'a self,
            _: &'a str,
            _: &'a ToolCall,
            _: u32,
        ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
            Box::pin(async { panic!("classification test must not dispatch") })
        }
    }

    #[test]
    fn review_regression_a_unknown_tool_never_acquires_external_read_budget() {
        assert_eq!(DynamicExecutor.tool_budget_class("unmapped_tool"), None);
        assert_eq!(
            DynamicExecutor.tool_budget_class("mcp_review_read"),
            Some(ToolBudgetClass::ExternalRead)
        );
    }

    #[test]
    fn review_regression_a_authorized_dynamic_mcp_retains_external_read_projection() {
        let mut policy = RunBudgetPolicy::standard();
        policy.max_external_read_tool_calls = 3;
        let tool = ToolSpec {
            name: "mcp_review_read".to_string(),
            description: "fixture".to_string(),
            input_schema: serde_json::json!({"type":"object"}),
            access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            max_results: None,
            capability_affinity: Vec::new(),
        };
        let projection = AgentToolLoop::from_policy(&policy).loop_projection(
            4,
            1,
            &HashMap::new(),
            &[tool],
            &DynamicExecutor,
        );
        assert!(
            projection.can_continue,
            "authorized dynamic read retains its budget category"
        );
    }
}
