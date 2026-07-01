import type { SkillListEntryDto } from "@/lib/ipc";

export function scopeLabel(scope: string): "global" | "vault" {
  return scope === "vault" ? "vault" : "global";
}

export function scopeText(scope: string): string {
  return scopeLabel(scope) === "vault" ? "Current vault" : "Global";
}

export function statusText(skill: SkillListEntryDto): string {
  if (!skill.enabled) return "Disabled";
  if (skill.confirmation_status === "needs_confirmation") {
    return "Needs confirmation";
  }
  if (skill.task_active === true) return "Matched";
  if (skill.activation_ready) return "Ready";
  return "Prompt-only";
}

export function confirmationState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  if (skill.confirmation_status === "confirmed") {
    return {
      label: "Confirmed",
      detail: "Prompt content has been confirmed.",
      needsAttention: false,
    };
  }
  return {
    label: "Needs confirmation",
    detail: "Confirm before prompt injection.",
    needsAttention: true,
  };
}

export function runtimeState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsAttention: boolean;
} {
  return {
    label:
      skill.kind === "legacy_prompt_only" ? "legacy prompt" : "prompt-only",
    detail: skill.activation_ready ? "Ready" : "Waiting for confirmation",
    needsAttention: !skill.activation_ready,
  };
}
