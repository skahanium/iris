import { FileText, Folder, Globe2, Hash, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import type { SkillListEntryDto } from "@/lib/ipc";

import { SkillStatusBadges } from "./SkillStatusBadges";
import { scopeLabel } from "./skill-status-state";

interface SkillCardProps {
  skill: SkillListEntryDto;
  sourceSummary: string;
  confirmation: {
    label: string;
    detail: string;
  };
  onUpdate: () => void;
  extraActions?: ReactNode;
}

export function SkillCard({
  skill,
  sourceSummary,
  confirmation,
  onUpdate,
  extraActions,
}: SkillCardProps) {
  const sc = scopeLabel(skill.scope);

  return (
    <div className="rounded-lg border border-border/70 bg-background px-3 py-3 transition-colors hover:border-border">
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
              <FileText className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">文件</span>
              <span className="truncate text-foreground/75">
                {skill.file_path}
              </span>
            </div>
            <div className="flex min-w-0 items-center gap-1.5">
              <Hash className="h-3.5 w-3.5 shrink-0" />
              <span className="shrink-0">内容哈希</span>
              <span className="truncate text-foreground/75">
                {skill.content_hash}
              </span>
            </div>
          </div>

          <div className="rounded-md border border-border/70 bg-muted/35 px-2.5 py-2">
            <div className="flex flex-wrap items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
              <span>确认状态</span>
              <span className="rounded-full border border-border/70 bg-background px-1.5 py-0.5 text-[10px] text-foreground/70">
                {confirmation.label}
              </span>
              <span className="rounded-full border border-border/70 bg-background px-1.5 py-0.5 text-[10px] text-foreground/70">
                {skill.activation_ready ? "可激活" : "等待确认"}
              </span>
            </div>
            <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
              {confirmation.detail}
            </p>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Tooltip content="刷新">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              aria-label="刷新"
              onClick={onUpdate}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          </Tooltip>
          {extraActions}
        </div>
      </div>
    </div>
  );
}
