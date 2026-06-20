use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ai_runtime::model_gateway::ToolCall;

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
    fn conflicts_with(&self, other: &Self) -> bool {
        self.resource_type == other.resource_type
            && self.resource_id == other.resource_id
            && self.access == ResourceAccess::Write
            && other.access == ResourceAccess::Write
    }
}

/// Explicit subagent execution contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubAgentTaskSpec {
    pub id: String,
    pub role: String,
    pub task: String,
    pub allowed_tools: Vec<String>,
    pub input_evidence_ids: Vec<String>,
    pub output_schema: String,
    pub resource_locks: Vec<ResourceLock>,
    pub token_budget: Option<u32>,
    pub failure_behavior: String,
}

impl SubAgentTaskSpec {
    pub fn from_tool_call(
        parent_request_id: &str,
        tool_call: &ToolCall,
        note_path: Option<&str>,
        input_evidence_ids: Vec<String>,
        inherited_allowed_tools: Vec<String>,
        token_budget: Option<u32>,
    ) -> Self {
        let args: serde_json::Value =
            serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::Value::Null);
        let task = args
            .get("task")
            .and_then(|value| value.as_str())
            .unwrap_or("subagent task")
            .to_string();
        let role = args
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("subagent")
            .to_string();
        let requested_allowed_tools = args
            .get("allowed_tools")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|items: &Vec<String>| !items.is_empty())
            .unwrap_or_default();
        let allowed_tools = if requested_allowed_tools.is_empty() {
            inherited_allowed_tools
        } else if inherited_allowed_tools.is_empty() {
            requested_allowed_tools
        } else {
            requested_allowed_tools
                .into_iter()
                .filter(|tool| inherited_allowed_tools.contains(tool))
                .collect()
        };
        let resource_locks = parse_resource_locks(&args).unwrap_or_else(|| {
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

        Self {
            id: if tool_call.id.is_empty() {
                format!("{parent_request_id}:subagent")
            } else {
                tool_call.id.clone()
            },
            role,
            task,
            allowed_tools,
            input_evidence_ids,
            output_schema: "SubagentReport".to_string(),
            resource_locks,
            token_budget,
            failure_behavior: "report_error".to_string(),
        }
    }
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

/// Resource conflict detected before launching subagents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationIssue {
    pub subagent_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub message: String,
}

/// Concurrency decision for a batch of subagents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationPlan {
    pub can_run_concurrently: bool,
    pub conflicts: Vec<CoordinationIssue>,
}

/// Unified subagent report surfaced to the parent harness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubagentReport {
    pub subagent_id: String,
    pub role: String,
    pub task: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
    pub open_questions: Vec<String>,
    pub errors: Vec<String>,
}

/// Stateless coordinator helpers for subagent launch and report normalization.
pub struct SubAgentCoordinator;

impl SubAgentCoordinator {
    pub fn plan(specs: &[SubAgentTaskSpec]) -> CoordinationPlan {
        let mut conflicts = Vec::new();
        for (left_index, left) in specs.iter().enumerate() {
            for right in specs.iter().skip(left_index + 1) {
                for left_lock in &left.resource_locks {
                    for right_lock in &right.resource_locks {
                        if left_lock.conflicts_with(right_lock) {
                            let message =
                                "subagent_resource_lock_conflict: same resource write lock"
                                    .to_string();
                            conflicts.push(CoordinationIssue {
                                subagent_id: left.id.clone(),
                                resource_type: left_lock.resource_type.clone(),
                                resource_id: left_lock.resource_id.clone(),
                                message: message.clone(),
                            });
                            conflicts.push(CoordinationIssue {
                                subagent_id: right.id.clone(),
                                resource_type: right_lock.resource_type.clone(),
                                resource_id: right_lock.resource_id.clone(),
                                message,
                            });
                        }
                    }
                }
            }
        }
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
        citation_valid: bool,
        harness_rounds: u32,
    ) -> SubagentReport {
        SubagentReport {
            subagent_id: spec.id.clone(),
            role: spec.role.clone(),
            task: spec.task.clone(),
            summary: summary.clone(),
            findings: if summary.is_empty() {
                Vec::new()
            } else {
                vec![summary]
            },
            evidence_ids: spec.input_evidence_ids.clone(),
            confidence: if citation_valid { 0.75 } else { 0.5 },
            open_questions: if harness_rounds == 0 {
                vec!["subagent returned without completing a harness round".to_string()]
            } else {
                Vec::new()
            },
            errors: Vec::new(),
        }
    }

    pub fn report_error(spec: &SubAgentTaskSpec, error: impl Into<String>) -> SubagentReport {
        SubagentReport {
            subagent_id: spec.id.clone(),
            role: spec.role.clone(),
            task: spec.task.clone(),
            summary: String::new(),
            findings: Vec::new(),
            evidence_ids: spec.input_evidence_ids.clone(),
            confidence: 0.0,
            open_questions: Vec::new(),
            errors: vec![error.into()],
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
        serde_json::json!({
            "content": report.summary,
            "citation_valid": report.errors.is_empty(),
            "harness_rounds": 0,
            "subagent_report": report,
        })
    }
}
