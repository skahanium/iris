import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function functionSlice(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("Agent Task Policy Phase B contract", () => {
  it("chat send main path derives policy before routing and never resolves SceneProfile", () => {
    const commands = read("src-tauri/src/commands/ai_commands.rs");
    const send = functionSlice(
      commands,
      "pub(crate) async fn execute_ai_send_message_with_routing",
      "/// Persist session + trace after a completed harness run",
    );

    expect(send).toContain("derive_task_policy_for_new_request(");
    expect(send).toContain("resolve_for_task_policy");
    expect(send).not.toContain("policy_for_legacy_chat_request(");
    expect(send).not.toContain("intent_from_legacy_scene");
    expect(send).not.toContain("resolve_scene(");
    expect(send).not.toContain("resolve_for_scene(");
    expect(send).not.toContain("slot_for_scene(");

    const helper = functionSlice(
      commands,
      "fn derive_task_policy_for_new_request",
      "fn budget_pause_checkpoint",
    );

    expect(helper).toContain("AgentTaskPolicy::from_input");
    expect(helper).not.toContain("AiScene");
    expect(helper).not.toContain("intent_from_legacy_scene");
  });

  it("harness loop planning is policy-driven, with scene retained only as legacy metadata", () => {
    const run = read("src-tauri/src/ai_harness/harness/run.rs");
    const body = functionSlice(
      run,
      "pub async fn run_harness",
      "fn abort_if_requested",
    );

    expect(body).toContain("input.task_policy");
    expect(body).toContain("resolve_max_rounds(&input.task_policy");
    expect(body).toContain("resolve_token_budget(&input.task_policy");
    expect(body).toContain("max_fetch_per_round(&input.task_policy)");
    expect(body).not.toContain("resolve_scene(");
    expect(body).not.toContain("slot_for_scene(");

    const reflection = read("src-tauri/src/ai_harness/harness/reflection.rs");
    expect(reflection).toContain("input.task_policy.max_agentic_rounds");
    expect(reflection).not.toContain("resolve_scene(");
  });

  it("policy module is the only new execution source for budgets, slots and task focus", () => {
    const policy = read("src-tauri/src/ai_runtime/agent_task_policy.rs");

    expect(policy).toContain("pub struct AgentTaskPolicy");
    expect(policy).toContain("pub struct AgentTaskPolicyInput");
    expect(policy).toContain("pub fn resolve_for_task_policy");
    expect(policy).toContain("pub fn task_focus");
    expect(policy).toContain("pub fn legacy_scene");
    expect(policy).not.toContain("ExemplarLearning =>");
  });

  it("policy context planner uses task intent and scope without legacy scene fallbacks", () => {
    const planner = read("src-tauri/src/ai_runtime/context_planner.rs");
    const policyPlan = functionSlice(
      planner,
      "pub fn plan_context_for_policy",
      "fn detect_intent_for_policy",
    );

    expect(policyPlan).toContain("policy.intent");
    expect(policyPlan).toContain("policy.scope");
    expect(policyPlan).not.toContain("legacy_scene");
    expect(policyPlan).not.toContain("AiScene");
  });

  it("persona main path exposes task focus instead of scene focus", () => {
    const persona = read("src-tauri/src/ai_runtime/persona_resolver.rs");
    const resolvedPersona = functionSlice(
      persona,
      "pub struct ResolvedPersona",
      "/// Resolve the effective persona from a user profile and scene context.",
    );
    const agentResolver = functionSlice(
      persona,
      "pub fn resolve_persona_for_agent",
      "/// Resolve persona layers from task policy.",
    );
    const policyResolver = functionSlice(
      persona,
      "pub fn resolve_persona_for_policy",
      "fn resolve_persona_for_task_focus",
    );

    expect(resolvedPersona).toContain("pub task_focus: String");
    expect(resolvedPersona).not.toContain("scene_focus");
    expect(agentResolver).toContain("task_focus(agent_intent");
    expect(agentResolver).not.toContain("agent_intent.scene()");
    expect(policyResolver).toContain("policy.task_focus()");
    expect(persona).not.toContain("resolve_scene_focus");
  });

  it("new AI command execution paths route through task policy, not scene routes", () => {
    for (const path of [
      "src-tauri/src/commands/research_commands.rs",
      "src-tauri/src/commands/document_commands.rs",
      "src-tauri/src/commands/writing_commands.rs",
    ]) {
      const source = read(path);
      expect(source).toContain("resolve_for_task_policy");
      expect(source).not.toContain("resolve_for_scene(");
      expect(source).not.toContain("to_provider_config(AiScene");
      expect(source).not.toContain("slot_for_scene(");
    }
  });

  it("LLM routing defaults no longer assert or save four scene routes", () => {
    const config = read("src-tauri/src/llm/config.rs");

    expect(config).not.toContain("default_routing_has_four_scenes");
    expect(config).not.toContain("assert_eq!(c.scenes.len(), 4)");
  });

  it("frontend chat execution passes task policy facts without letting legacy scene override routing", () => {
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
    const executeKnowledgeChat = functionSlice(
      tasks,
      "const executeKnowledgeChat = useCallback",
      "const runKnowledgeChat = useCallback",
    );

    expect(executeKnowledgeChat).toContain("agentIntent");
    expect(executeKnowledgeChat).toContain("taskPlan");
    expect(executeKnowledgeChat).toContain("intentDetection");
    expect(executeKnowledgeChat).toContain("assistantExecute({");
    expect(executeKnowledgeChat).not.toContain("legacySceneHintForAgentIntent");
  });

  it("capability slot selection does not inspect legacy scene", () => {
    const config = read("src-tauri/src/llm/config.rs");
    const requestedSlot = functionSlice(
      config,
      "fn requested_slot",
      "fn fallback_chain_for",
    );

    expect(requestedSlot).toContain("input.intent");
    expect(requestedSlot).not.toContain("AiScene");
    expect(requestedSlot).not.toContain("scene");
    expect(requestedSlot).not.toContain("slot_for_legacy_scene");
  });
});
