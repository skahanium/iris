import {
  AlertTriangle,
  Folder,
  Globe2,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import type { SkillListEntryDto } from "@/lib/ipc";

import { SkillStatusBadges } from "./SkillStatusBadges";
import { scopeLabel } from "./skill-status-state";

interface CapabilityGroup {
  label: string;
  tone: "calm" | "info" | "warn" | "danger";
}

interface SkillCardProps {
  skill: SkillListEntryDto;
  sourceSummary: string;
  confirmation: {
    label: string;
    detail: string;
  };
  sections: {
    activated: string[];
    blocked: string[];
  };
  capabilityGroups: CapabilityGroup[];
  capabilityToneClass: (tone: CapabilityGroup["tone"]) => string;
  capabilitySummaryLabel: string;
  criticalBlocked: boolean;
  compatibilityWarning: boolean;
  onUpdate: () => void;
  extraActions?: ReactNode;
}

export function SkillCard({
  skill,
  sourceSummary,
  confirmation,
  sections,
  capabilityGroups,
  capabilityToneClass,
  capabilitySummaryLabel,
  criticalBlocked,
  compatibilityWarning,
  onUpdate,
  extraActions,
}: SkillCardProps) {
  const sc = scopeLabel(skill.scope);

  return (
    <div className="rounded-lg border border-border/70 bg-background px-3 py-3 shadow-sm transition-colors hover:border-border">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate text-sm font-medium">{skill.name}</p>
            <SkillStatusBadges skill={skill} />
          </div>

          {skill.description ? (
            <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">
              {skill.description}
            </p>
          ) : null}

          <div className="grid gap-1 text-[11px] text-muted-foreground">
            <div className="flex min-w-0 items-center gap-1.5">
              {sc === "vault" ? (
                <Folder className="h-3.5 w-3.5 shrink-0" />
              ) : (
                <Globe2 className="h-3.5 w-3.5 shrink-0" />
              )}
              <span className="shrink-0">来源</span>
              <span className="truncate text-foreground/75">
                {sourceSummary}
              </span>
            </div>
            <div className="flex min-w-0 items-center gap-1.5">
              <Folder className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">文件路径</span>
              <span className="truncate text-foreground/75">
                {skill.file_path}
              </span>
            </div>
          </div>

          <div className="rounded-md border border-border/70 bg-muted/35 px-2.5 py-2">
            <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
              <ShieldCheck className="h-3.5 w-3.5" />
              <span>确认状态</span>
              <span className="rounded-full border border-border/70 bg-background px-1.5 py-0.5 text-[10px] text-foreground/70">
                {confirmation.label}
              </span>
            </div>
            <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
              {confirmation.detail}
            </p>
          </div>

          {sections.activated.length > 0 || sections.blocked.length > 0 ? (
            <div className="rounded-md border border-border/70 bg-muted/35 px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
              {sections.activated.length > 0 ? (
                <p>
                  <span className="font-medium text-foreground/75">
                    可用片段
                  </span>
                  <span className="ml-1">{sections.activated.join(", ")}</span>
                </p>
              ) : null}
              {sections.blocked.length > 0 ? (
                <p>
                  <span className="font-medium text-foreground/75">
                    阻塞片段
                  </span>
                  <span className="ml-1">{sections.blocked.join(", ")}</span>
                </p>
              ) : null}
            </div>
          ) : null}

          <div className="space-y-1.5">
            <div className="flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
              <ShieldCheck className="h-3.5 w-3.5" />
              <span>{capabilitySummaryLabel}</span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              {capabilityGroups.map((group) => (
                <span
                  key={group.label}
                  className={`rounded-full border px-2 py-0.5 text-[10px] ${capabilityToneClass(group.tone)}`}
                >
                  {group.label}
                </span>
              ))}
            </div>
          </div>

          {criticalBlocked ? (
            <div className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-2.5 py-2 text-[11px] leading-5 text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-200">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>
                包含当前不支持的高风险能力，已默认禁用。需要处理后才能启用。
              </span>
            </div>
          ) : compatibilityWarning ? (
            <div className="flex items-start gap-2 rounded-md border border-border/70 bg-muted/45 px-2.5 py-2 text-[11px] leading-5 text-muted-foreground">
              <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span>存在兼容性提示；真正执行时仍会逐次弹出工具确认。</span>
            </div>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title="刷新"
            onClick={onUpdate}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
          {extraActions}
        </div>
      </div>
    </div>
  );
}
