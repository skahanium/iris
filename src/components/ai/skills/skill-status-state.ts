import type { SkillListEntryDto } from "@/lib/ipc";

export function scopeLabel(scope: string): "global" | "vault" {
  return scope === "vault" ? "vault" : "global";
}

export function scopeText(scope: string): string {
  return scopeLabel(scope) === "vault" ? "当前库" : "全局";
}

export function statusText(skill: SkillListEntryDto): string {
  if (!skill.enabled) return "已禁用";
  if (skill.confirmation_status === "needs_confirmation") {
    return "待确认";
  }
  if (skill.task_active === true) return "已匹配";
  if (skill.activation_ready) return "就绪";
  return "纯提示词";
}

export function confirmationState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  if (skill.confirmation_status === "confirmed") {
    return {
      label: "已确认",
      detail: "提示词内容已确认。",
      needsAttention: false,
    };
  }
  return {
    label: "待确认",
    detail: "注入提示词前需先确认。",
    needsAttention: true,
  };
}

export function runtimeState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  return {
    label: skill.kind === "legacy_prompt_only" ? "旧版提示词" : "纯提示词",
    detail: skill.activation_ready ? "就绪" : "等待确认",
    needsAttention: !skill.activation_ready,
  };
}
