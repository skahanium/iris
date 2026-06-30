import type { SkillListEntryDto } from "@/lib/ipc";

export function scopeLabel(scope: string): "global" | "vault" {
  return scope === "vault" ? "vault" : "global";
}

export function scopeText(scope: string): string {
  return scopeLabel(scope) === "vault" ? "当前库" : "全局";
}

export function statusText(skill: SkillListEntryDto): string {
  if (!skill.enabled) return "已禁用";
  if (skill.task_active === true) return "本次匹配";
  if (skill.task_active === false) return "已启用";
  return skill.availability === "partial" ? "部分可用" : "已启用";
}

export function runtimeState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  const deps = skill.mcp_dependencies ?? [];
  const status =
    skill.runtime_status ?? (skill.runtime_ready ? "ready" : "unknown");

  if (skill.runtime_kind === "mcp" || deps.length > 0) {
    const detail = deps.length > 0 ? deps.join(", ") : "MCP profile";
    return {
      label: skill.runtime_ready ? "MCP 已就绪" : "MCP 未就绪",
      detail: skill.runtime_ready
        ? detail
        : `缺少或未启用 MCP profile${deps.length > 0 ? `：${detail}` : ""}`,
      needsAttention: !skill.runtime_ready,
    };
  }

  if (
    status === "degraded" ||
    status === "blocked" ||
    status === "unavailable"
  ) {
    return {
      label: status,
      detail:
        (skill.degraded_reasons ?? []).slice(0, 2).join("; ") || skill.kind,
      needsAttention: true,
    };
  }

  return {
    label: skill.kind === "legacy_prompt_only" ? "prompt-only" : skill.kind,
    detail: "不需要运行时",
    needsAttention: false,
  };
}
