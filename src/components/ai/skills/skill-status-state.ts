import type { SkillListEntryDto } from "@/lib/ipc";

export function scopeLabel(scope: string): "global" | "vault" {
  return scope === "vault" ? "vault" : "global";
}

export function scopeText(scope: string): string {
  return scopeLabel(scope) === "vault" ? "当前库" : "全局";
}

export function statusText(skill: SkillListEntryDto): string {
  if (!skill.enabled) return "已禁用";
  if (skill.confirmation_status === "needs_confirmation") return "需要确认";
  if (skill.task_active === true) return "本次匹配";
  if (skill.task_active === false) return "已启用";
  return skill.availability === "partial" ? "部分可用" : "已启用";
}

export function confirmationState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  if (skill.confirmation_status === "confirmed") {
    return {
      label: "已确认",
      detail: "当前 SKILL.md 内容已确认。",
      needsAttention: false,
    };
  }
  return {
    label: "需要确认",
    detail: "确认后才会参与提示注入。",
    needsAttention: true,
  };
}

export function runtimeState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  if (skill.degraded_reasons.length > 0 || skill.blocked_sections.length > 0) {
    return {
      label: "prompt-only 部分可用",
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
