use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ai_runtime::model_gateway::ToolCall;

pub(crate) const MAX_SUBAGENT_BATCH_TASKS: usize = 3;
const MAX_SUBAGENT_SUMMARY_CHARS: usize = 600;

/// Resource access requested by a subagent task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccess {
    Read,
    Write,
}

impl ResourceAccess {
    fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("write") {
            Self::Write
        } else {
            Self::Read
        }
    }
}

/// Bounded resource lock declaration for a subagent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLock {
    pub resource_type: String,
    pub resource_id: String,
    pub access: ResourceAccess,
}

impl ResourceLock {
    fn rejects_read_only_child_run(&self) -> bool {
        self.access == ResourceAccess::Write
    }
}

/// Explicit subagent execution contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubAgentTaskSpec {
    pub id: String,
    pub role: String,
    pub task: String,
    pub allowed_tools: Vec<String>,
    pub output_schema: String,
    pub resource_locks: Vec<ResourceLock>,
    pub token_budget: Option<u32>,
    pub failure_behavior: String,
}

impl SubAgentTaskSpec {
    /// Build the legacy single-task contract.
    pub fn from_tool_call(
        parent_request_id: &str,
        tool_call: &ToolCall,
        note_path: Option<&str>,
        inherited_allowed_tools: Vec<String>,
        token_budget: Option<u32>,
    ) -> Self {
        let args: serde_json::Value =
            serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::Value::Null);
        Self::from_task_args(
            subagent_call_id(parent_request_id, tool_call),
            &args,
            note_path,
            inherited_allowed_tools,
            token_budget,
        )
        .unwrap_or_else(|_| Self {
            id: subagent_call_id(parent_request_id, tool_call),
            role: "subagent".to_string(),
            task: "subagent task".to_string(),
            allowed_tools: Vec::new(),
            output_schema: "SubagentReport".to_string(),
            resource_locks: Vec::new(),
            token_budget,
            failure_behavior: "report_error".to_string(),
        })
    }

    pub(crate) fn batch_from_tool_call(
        parent_request_id: &str,
        tool_call: &ToolCall,
        args: &serde_json::Value,
        note_path: Option<&str>,
        inherited_allowed_tools: Vec<String>,
        token_budget: Option<u32>,
    ) -> Result<Vec<Self>, &'static str> {
        let single_task = args.get("task");
        let batch_tasks = args.get("tasks");
        match (single_task, batch_tasks) {
            (Some(_), Some(_)) => Err("child_run_task_and_tasks_mutually_exclusive"),
            (None, None) => Err("child_run_task_required"),
            (Some(_), None) => Ok(vec![Self::from_task_args(
                subagent_call_id(parent_request_id, tool_call),
                args,
                note_path,
                inherited_allowed_tools,
                token_budget,
            )?]),
            (None, Some(tasks)) => {
                let tasks = tasks.as_array().ok_or("child_run_batch_tasks_invalid")?;
                if tasks.is_empty() {
                    return Err("child_run_batch_empty");
                }
                if tasks.len() > MAX_SUBAGENT_BATCH_TASKS {
                    return Err("child_run_batch_limit_exceeded");
                }
                let base_id = subagent_call_id(parent_request_id, tool_call);
                tasks
                    .iter()
                    .enumerate()
                    .map(|(index, task_args)| {
                        Self::from_task_args(
                            format!("{base_id}:{}", index + 1),
                            task_args,
                            note_path,
                            inherited_allowed_tools.clone(),
                            token_budget,
                        )
                    })
                    .collect()
            }
        }
    }

    fn from_task_args(
        id: String,
        args: &serde_json::Value,
        note_path: Option<&str>,
        inherited_allowed_tools: Vec<String>,
        token_budget: Option<u32>,
    ) -> Result<Self, &'static str> {
        let task = args
            .get("task")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|task| !task.is_empty())
            .ok_or("child_run_task_invalid")?
            .to_string();
        let role = args
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("subagent")
            .to_string();
        let allowed_tools = match args.get("allowed_tools") {
            None => inherited_allowed_tools,
            Some(value) => value
                .as_array()
                .ok_or("child_run_allowed_tools_invalid")?
                .iter()
                .map(|item| item.as_str().ok_or("child_run_allowed_tools_invalid"))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|tool| inherited_allowed_tools.iter().any(|parent| parent == tool))
                .map(str::to_string)
                .collect(),
        };
        let resource_locks = parse_resource_locks(args).unwrap_or_else(|| {
            note_path
                .map(|path| {
                    vec![ResourceLock {
                        resource_type: "note".to_string(),
                        resource_id: path.to_string(),
                        access: ResourceAccess::Read,
                    }]
                })
                .unwrap_or_default()
        });

        Ok(Self {
            id,
            role,
            task,
            allowed_tools,
            output_schema: "SubagentReport".to_string(),
            resource_locks,
            token_budget,
            failure_behavior: "report_error".to_string(),
        })
    }
}

fn subagent_call_id(parent_request_id: &str, tool_call: &ToolCall) -> String {
    let raw = if tool_call.id.is_empty() {
        format!("{parent_request_id}:subagent")
    } else {
        tool_call.id.clone()
    };
    bounded_subagent_id(raw)
}

fn bounded_subagent_id(raw: String) -> String {
    if raw.chars().count() <= 160
        && !raw.chars().any(char::is_control)
        && !crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(&raw)
    {
        raw
    } else {
        format!("subagent:{}", crate::cas::hash::content_hash_str(&raw))
    }
}

fn safe_report_summary(value: &str) -> String {
    let trimmed = value.trim();
    if crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(trimmed) {
        return "敏感内容已隐藏".to_string();
    }
    trimmed
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(MAX_SUBAGENT_SUMMARY_CHARS)
        .collect()
}

fn parse_resource_locks(args: &serde_json::Value) -> Option<Vec<ResourceLock>> {
    let raw_locks = args.get("resource_locks")?.as_array()?;
    let locks = raw_locks
        .iter()
        .map(|item| {
            let access = item
                .get("access")
                .and_then(|value| value.as_str())
                .map(ResourceAccess::parse)
                .unwrap_or(ResourceAccess::Read);
            if let Some(resource) = item.get("resource").and_then(|value| value.as_str()) {
                let (resource_type, resource_id) = resource
                    .split_once(':')
                    .map(|(kind, id)| (kind.to_string(), id.to_string()))
                    .unwrap_or_else(|| ("note".to_string(), resource.to_string()));
                return ResourceLock {
                    resource_type,
                    resource_id,
                    access,
                };
            }
            ResourceLock {
                resource_type: item
                    .get("resource_type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("note")
                    .to_string(),
                resource_id: item
                    .get("resource_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                access,
            }
        })
        .filter(|lock| !lock.resource_id.is_empty())
        .collect::<Vec<_>>();
    Some(locks)
}

/// Read-only policy violation detected before launching subagents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationIssue {
    pub subagent_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub message: String,
}

/// Read-only admission decision for a batch of subagents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationPlan {
    pub can_run_concurrently: bool,
    pub conflicts: Vec<CoordinationIssue>,
}

/// Harness-generated budget usage for one bounded ChildRun.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentBudgetUsage {
    pub model_turns: u32,
    pub tool_calls: u32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Unified subagent report surfaced to the parent harness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentReport {
    pub subagent_id: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub evidence_ids: Vec<String>,
    /// Harness-generated confidence percentage in the closed interval 0..=100.
    pub confidence: u8,
    pub open_questions: Vec<String>,
    pub errors: Vec<String>,
    pub budget: SubagentBudgetUsage,
}

/// Request-ordered reports for one bounded ChildRun batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentBatchReport {
    pub items: Vec<SubagentReport>,
}

/// Stateless coordinator helpers for subagent launch and report normalization.
pub struct SubAgentCoordinator;

impl SubAgentCoordinator {
    /// Restrict a child Run to the read/Web vocabulary that may safely execute
    /// under its parent Run. Child Runs never receive mutation tools or harness
    /// controls, so depth cannot grow beyond one and every durable effect stays
    /// on the parent confirmation path.
    pub(crate) fn child_tool_surface(parent_tools: &[String]) -> Vec<String> {
        const CHILD_SAFE_TOOLS: &[&str] = &[
            "search_hybrid",
            "search_semantic",
            "search_keyword",
            "get_regulation",
            "get_context_packets",
            "system_time_now",
            "app_context_read",
            "capabilities_read",
            "web_search",
            "read_note",
            "list_vault",
            "get_outline",
            "get_backlinks",
            "get_block_links",
            "vault_version_list",
            "git_read_status",
            "git_read_diff",
            "git_read_log",
            "doc_extract_citations",
        ];
        parent_tools
            .iter()
            .filter(|tool| CHILD_SAFE_TOOLS.contains(&tool.as_str()))
            .cloned()
            .collect()
    }

    pub fn plan(specs: &[SubAgentTaskSpec]) -> CoordinationPlan {
        let mut conflicts = specs
            .iter()
            .flat_map(|spec| {
                spec.resource_locks
                    .iter()
                    .filter(|lock| lock.rejects_read_only_child_run())
                    .map(|lock| CoordinationIssue {
                        subagent_id: spec.id.clone(),
                        resource_type: lock.resource_type.clone(),
                        resource_id: lock.resource_id.clone(),
                        message: "child_run_write_lock_forbidden".to_string(),
                    })
            })
            .collect::<Vec<_>>();
        conflicts.sort_by(|a, b| {
            a.subagent_id
                .cmp(&b.subagent_id)
                .then_with(|| a.resource_id.cmp(&b.resource_id))
        });
        conflicts.dedup_by(|a, b| {
            a.subagent_id == b.subagent_id
                && a.resource_type == b.resource_type
                && a.resource_id == b.resource_id
        });
        CoordinationPlan {
            can_run_concurrently: conflicts.is_empty(),
            conflicts,
        }
    }

    pub fn report_success(
        spec: &SubAgentTaskSpec,
        summary: String,
        mut evidence_ids: Vec<String>,
        budget: SubagentBudgetUsage,
    ) -> SubagentReport {
        evidence_ids.sort();
        evidence_ids.dedup();
        let summary = safe_report_summary(&summary);
        SubagentReport {
            subagent_id: spec.id.clone(),
            summary,
            findings: Vec::new(),
            confidence: if evidence_ids.is_empty() { 50 } else { 75 },
            evidence_ids,
            open_questions: Vec::new(),
            errors: Vec::new(),
            budget,
        }
    }

    /// Build a structured failure without copying provider text into the report.
    pub fn report_error(spec: &SubAgentTaskSpec, error: impl Into<String>) -> SubagentReport {
        Self::report_error_with_budget(spec, error, SubagentBudgetUsage::default(), Vec::new())
    }

    /// Build a structured failure while retaining the budget already consumed.
    pub(crate) fn report_error_with_budget(
        spec: &SubAgentTaskSpec,
        error: impl Into<String>,
        budget: SubagentBudgetUsage,
        mut evidence_ids: Vec<String>,
    ) -> SubagentReport {
        evidence_ids.sort();
        evidence_ids.dedup();
        SubagentReport {
            subagent_id: spec.id.clone(),
            summary: String::new(),
            findings: Vec::new(),
            evidence_ids,
            confidence: 0,
            open_questions: Vec::new(),
            errors: vec![error.into()],
            budget,
        }
    }

    pub fn conflict_errors_by_subagent(
        plan: &CoordinationPlan,
    ) -> HashMap<String, Vec<CoordinationIssue>> {
        let mut grouped: HashMap<String, Vec<CoordinationIssue>> = HashMap::new();
        for issue in &plan.conflicts {
            grouped
                .entry(issue.subagent_id.clone())
                .or_default()
                .push(issue.clone());
        }
        grouped
    }

    pub fn tool_output_for_report(report: &SubagentReport) -> serde_json::Value {
        Self::tool_output_for_batch(&SubagentBatchReport {
            items: vec![report.clone()],
        })
    }

    pub(crate) fn invalid_batch_report(
        subagent_id: impl Into<String>,
        error: impl Into<String>,
    ) -> SubagentBatchReport {
        SubagentBatchReport {
            items: vec![SubagentReport {
                subagent_id: bounded_subagent_id(subagent_id.into()),
                summary: String::new(),
                findings: Vec::new(),
                evidence_ids: Vec::new(),
                confidence: 0,
                open_questions: Vec::new(),
                errors: vec![error.into()],
                budget: SubagentBudgetUsage::default(),
            }],
        }
    }

    pub(crate) fn tool_output_for_batch(report: &SubagentBatchReport) -> serde_json::Value {
        serde_json::json!({
            "subagentBatchReport": report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::model_gateway::FunctionCall;

    fn subagent_call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call-sub".to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: "spawn_subagent".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn requested_allowed_tools_are_intersected_with_parent_surface() {
        let call = subagent_call(r#"{"task":"search","allowed_tools":["web_search","read_note"]}"#);

        let spec = SubAgentTaskSpec::from_tool_call(
            "parent",
            &call,
            None,
            vec!["read_note".to_string()],
            Some(1000),
        );

        assert_eq!(spec.allowed_tools, vec!["read_note".to_string()]);
    }

    #[test]
    fn requested_tools_do_not_expand_empty_parent_surface() {
        let call = subagent_call(r#"{"task":"search","allowed_tools":["web_search"]}"#);

        let spec = SubAgentTaskSpec::from_tool_call("parent", &call, None, Vec::new(), Some(1000));

        assert!(spec.allowed_tools.is_empty());
    }

    #[test]
    fn explicit_empty_allowed_tools_does_not_inherit_parent_surface() {
        let call = subagent_call(r#"{"task":"reason only","allowed_tools":[]}"#);

        let spec = SubAgentTaskSpec::from_tool_call(
            "parent",
            &call,
            None,
            vec!["read_note".to_string(), "web_search".to_string()],
            Some(1000),
        );

        assert!(
            spec.allowed_tools.is_empty(),
            "an explicit empty list must remain a tool-free child surface"
        );
    }

    #[test]
    fn child_reports_never_copy_parent_evidence() {
        let call = subagent_call(r#"{"task":"reason only"}"#);
        let spec = SubAgentTaskSpec::from_tool_call(
            "parent",
            &call,
            None,
            vec!["read_note".to_string()],
            Some(1000),
        );

        let success = SubAgentCoordinator::report_success(
            &spec,
            "done".to_string(),
            Vec::new(),
            SubagentBudgetUsage::default(),
        );
        let failure = SubAgentCoordinator::report_error(&spec, "child_run_failed");

        assert!(success.evidence_ids.is_empty());
        assert!(failure.evidence_ids.is_empty());
        assert_eq!(success.confidence, 50);
    }

    #[test]
    fn failed_child_report_keeps_only_evidence_registered_by_that_child() {
        let call = subagent_call(r#"{"task":"search then verify"}"#);
        let spec = SubAgentTaskSpec::from_tool_call(
            "parent",
            &call,
            None,
            vec!["web_search".to_string()],
            Some(1000),
        );

        let report = SubAgentCoordinator::report_error_with_budget(
            &spec,
            "child_run_failed",
            SubagentBudgetUsage {
                model_turns: 2,
                tool_calls: 1,
                ..Default::default()
            },
            vec!["42".to_string()],
        );

        assert_eq!(report.evidence_ids, vec!["42"]);
        assert_eq!(report.confidence, 0);
        assert_eq!(report.budget.tool_calls, 1);
    }

    #[test]
    fn persisted_report_summary_redacts_credential_shaped_content() {
        let call = subagent_call(r#"{"task":"check"}"#);
        let spec = SubAgentTaskSpec::from_tool_call("parent", &call, None, Vec::new(), Some(1000));

        for leaked in [
            "api_key=plain-secret",
            "token=plain-secret",
            "key=plain-secret",
            "password: plain-secret",
            r#"{"token":"plain-secret"}"#,
            "token: plain-secret",
            r#"{"token" : "plain-secret"}"#,
            "x-api-key: plain-secret",
            "client_secret: plain-secret",
        ] {
            let report = SubAgentCoordinator::report_success(
                &spec,
                leaked.to_string(),
                Vec::new(),
                SubagentBudgetUsage::default(),
            );

            assert_eq!(report.summary, "敏感内容已隐藏");
            assert!(!serde_json::to_string(&report)
                .expect("serialize report")
                .contains("plain-secret"));
        }
    }

    #[test]
    fn batch_parser_requires_exactly_one_bounded_task_shape() {
        let inherited = vec!["read_note".to_string()];
        let parse = |arguments: &str| {
            let call = subagent_call(arguments);
            let args: serde_json::Value = serde_json::from_str(arguments).expect("arguments");
            SubAgentTaskSpec::batch_from_tool_call(
                "parent",
                &call,
                &args,
                None,
                inherited.clone(),
                Some(2_000),
            )
        };

        assert_eq!(
            parse(r#"{"task":"one","tasks":[{"task":"two"}]}"#)
                .expect_err("task and tasks must be exclusive"),
            "child_run_task_and_tasks_mutually_exclusive"
        );
        assert_eq!(
            parse(r#"{"tasks":[]}"#).expect_err("empty batch"),
            "child_run_batch_empty"
        );
        assert_eq!(
            parse(r#"{"tasks":[{"task":"one"},{"task":"two"},{"task":"three"},{"task":"four"}]}"#)
                .expect_err("bounded batch"),
            "child_run_batch_limit_exceeded"
        );

        let specs = parse(r#"{"tasks":[{"task":"one"},{"task":"two"},{"task":"three"}]}"#)
            .expect("bounded batch");
        assert_eq!(
            specs
                .iter()
                .map(|spec| spec.id.as_str())
                .collect::<Vec<_>>(),
            vec!["call-sub:1", "call-sub:2", "call-sub:3"]
        );
    }

    #[test]
    fn child_tool_surface_never_contains_mutation_or_recursive_harness_controls() {
        let tools = SubAgentCoordinator::child_tool_surface(&[
            "read_note".to_string(),
            "web_search".to_string(),
            "memory_write".to_string(),
            "insert_text_at_cursor".to_string(),
            "spawn_subagent".to_string(),
            "conclude_reasoning".to_string(),
        ]);

        assert_eq!(tools, vec!["read_note", "web_search"]);
    }

    #[test]
    fn spawn_subagent_catalog_declares_single_or_bounded_batch_tasks() {
        let entry = crate::ai_runtime::tool_catalog::catalog_find("spawn_subagent")
            .expect("spawn_subagent catalog entry");
        let properties = entry.input_schema["properties"]
            .as_object()
            .expect("object properties");
        let tasks = properties
            .get("tasks")
            .expect("batch tasks property")
            .as_object()
            .expect("tasks schema");

        assert!(properties.contains_key("task"));
        assert_eq!(tasks.get("type"), Some(&serde_json::json!("array")));
        assert_eq!(tasks.get("maxItems"), Some(&serde_json::json!(3)));
        assert_eq!(
            tasks["items"]["required"],
            serde_json::json!(["task"]),
            "each batch item must carry its own task"
        );
        assert!(
            entry.input_schema.get("required").is_none(),
            "task and tasks are mutually exclusive alternatives, so neither is globally required"
        );
    }
}
