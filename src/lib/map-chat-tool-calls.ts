import type { ToolCallInfo, ToolCallStatus } from "@/types/ai";

/** Raw tool_call shape from Rust / OpenAI (nested `function`). */
interface RawToolCall {
  id: string;
  name?: string;
  function?: { name: string; arguments?: string };
}

interface RawToolResult {
  tool_call_id: string;
  status: string;
  result?: unknown;
  error?: string;
}

function parseArguments(raw: RawToolCall): Record<string, unknown> | undefined {
  const argsStr = raw.function?.arguments;
  if (!argsStr) return undefined;
  try {
    const parsed: unknown = JSON.parse(argsStr);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch {
    return undefined;
  }
  return undefined;
}

function statusFromResult(status: string | undefined): ToolCallStatus {
  switch (status) {
    case "completed":
    case "executed":
      return "completed";
    case "pending_confirmation":
      return "pending";
    case "error":
      return "failed";
    case "rejected":
      return "rejected";
    default:
      return "pending";
  }
}

function summarizeSubagentResult(result: unknown): string | undefined {
  if (!result || typeof result !== "object") return undefined;
  const r = result as { content?: string; error?: string };
  if (typeof r.error === "string" && r.error) {
    return r.error.slice(0, 400);
  }
  if (typeof r.content === "string" && r.content.trim()) {
    const t = r.content.trim();
    return t.length > 400 ? `${t.slice(0, 400)}…` : t;
  }
  return undefined;
}

function isVisibleTool(tc: ToolCallInfo): boolean {
  if (tc.name === "spawn_subagent" || tc.name === "conclude_reasoning") {
    return true;
  }
  return tc.status === "pending" || tc.status === "failed";
}

/**
 * Map harness chat response tool_calls + tool_results to UI bubbles.
 * Hides auto-completed read-only tools (already reflected in the answer).
 * Always shows sub-agent and conclude_reasoning with summaries when available.
 */
export function mapChatToolCallsForUi(
  toolCalls: RawToolCall[] | undefined,
  toolResults: RawToolResult[] | undefined,
): ToolCallInfo[] | undefined {
  if (!toolCalls?.length) return undefined;

  const resultById = new Map(
    (toolResults ?? []).map((r) => [r.tool_call_id, r]),
  );

  const mapped: ToolCallInfo[] = toolCalls.map((tc) => {
    const id = tc.id;
    const name = tc.name ?? tc.function?.name ?? "tool";
    const tr = resultById.get(id);
    const status = statusFromResult(tr?.status);
    let result_summary: string | undefined;
    if (name === "spawn_subagent" && tr?.result !== undefined) {
      result_summary = summarizeSubagentResult(tr.result);
    }
    return {
      id,
      name,
      arguments: parseArguments(tc),
      status,
      result_summary,
      error:
        status === "failed" && tr && "error" in tr
          ? String((tr as { error?: string }).error ?? "")
          : undefined,
    };
  });

  const visible = mapped.filter(isVisibleTool);

  return visible.length > 0 ? visible : undefined;
}
