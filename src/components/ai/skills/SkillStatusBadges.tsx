import type { SkillListEntryDto } from "@/lib/ipc";

import { runtimeState, scopeText, statusText } from "./skill-status-state";

export function SkillStatusBadges({ skill }: { skill: SkillListEntryDto }) {
  const legacy = Boolean(skill.legacy_trigger);
  const invalid =
    typeof skill.validation === "object" && "invalid" in skill.validation;
  const runtime = runtimeState(skill);

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
            ? "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-300"
            : "border-border/70 bg-background text-muted-foreground"
        }`}
      >
        {runtime.label}
      </span>
      {legacy ? (
        <span className="rounded-full border border-amber-200 bg-amber-50 px-2 py-0.5 text-[10px] text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-300">
          旧格式
        </span>
      ) : null}
      {invalid ? (
        <span className="rounded-full border border-red-200 bg-red-50 px-2 py-0.5 text-[10px] text-red-700 dark:border-red-900/60 dark:bg-red-950/35 dark:text-red-300">
          需要修复
        </span>
      ) : null}
    </div>
  );
}
