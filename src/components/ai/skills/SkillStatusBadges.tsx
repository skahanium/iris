import type { SkillListEntryDto } from "@/lib/ipc";

import {
  confirmationState,
  runtimeState,
  scopeText,
  statusText,
} from "./skill-status-state";

export function SkillStatusBadges({ skill }: { skill: SkillListEntryDto }) {
  const legacy = Boolean(skill.legacy_trigger);
  const invalid =
    typeof skill.validation === "object" && "invalid" in skill.validation;
  const runtime = runtimeState(skill);
  const confirmation = confirmationState(skill);

  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="rounded-full border border-border/70 bg-muted/60 px-2 py-0.5 text-[10px] text-muted-foreground">
        {scopeText(skill.scope)}
      </span>
      <span className="rounded-full border border-border/70 bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
        {statusText(skill)}
      </span>
      <span
        className={`rounded-full border px-2 py-0.5 text-[10px] ${
          runtime.needsAttention
            ? "border-warning/30 bg-warning-bg text-warning-foreground"
            : "border-border/70 bg-background text-muted-foreground"
        }`}
      >
        {runtime.label}
      </span>
      <span
        className={`rounded-full border px-2 py-0.5 text-[10px] ${
          confirmation.needsAttention
            ? "border-warning/30 bg-warning-bg text-warning-foreground"
            : "border-border/70 bg-background text-muted-foreground"
        }`}
      >
        {confirmation.label}
      </span>
      {legacy ? (
        <span className="rounded-full border border-warning/30 bg-warning-bg px-2 py-0.5 text-[10px] text-warning-foreground">
          旧格式
        </span>
      ) : null}
      {invalid ? (
        <span className="rounded-full border border-destructive/30 bg-destructive/10 px-2 py-0.5 text-[10px] text-destructive">
          需要修复
        </span>
      ) : null}
    </div>
  );
}
