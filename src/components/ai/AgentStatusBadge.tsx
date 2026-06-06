import {
  Activity,
  Globe,
  Lock,
  Puzzle,
  Shield,
  Wrench,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { skillsList, type SkillListEntryDto } from "@/lib/ipc";

interface AgentStatusBadgeProps {
  webSearchEnabled?: boolean;
  disabled?: boolean;
  onOpenSkills?: () => void;
  auditAvailable?: boolean;
  onOpenAudit?: () => void;
}

function PolicyRow({
  icon: Icon,
  label,
  detail,
  accent,
}: {
  icon: typeof Wrench;
  label: string;
  detail: string;
  accent?: "success" | "muted";
}) {
  return (
    <div className="flex items-start gap-2.5 rounded-md px-2 py-1.5">
      <span
        className={cn(
          "mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-muted/60",
          accent === "success" && "text-primary",
        )}
      >
        <Icon className="h-3.5 w-3.5" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium text-foreground">{label}</p>
        <p className="text-[10px] leading-relaxed text-muted-foreground">
          {detail}
        </p>
      </div>
    </div>
  );
}

export function AgentStatusBadge({
  webSearchEnabled = false,
  disabled,
  onOpenSkills,
  auditAvailable = false,
  onOpenAudit,
}: AgentStatusBadgeProps) {
  const [skills, setSkills] = useState<SkillListEntryDto[]>([]);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    skillsList()
      .then(setSkills)
      .catch(() => setSkills([]));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  const enabledSkills = skills.filter((s) => s.enabled);
  const hasSkills = enabledSkills.length > 0;

  const close = useCallback(() => setOpen(false), []);

  return (
    <div className="relative" ref={containerRef}>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-8 shrink-0 gap-1 px-2 text-xs"
        title="Agent 状态"
        disabled={disabled}
        data-testid="agent-status-trigger"
        onClick={() => setOpen((v) => !v)}
      >
        <Activity className="h-3.5 w-3.5" />
        状态
      </Button>

      {open ? (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-72 rounded-md border border-border bg-popover shadow-md"
          data-testid="agent-status-popover"
        >
          <div className="border-b border-border/60 px-3 py-2.5">
            <p className="text-xs font-medium text-foreground">Agent 状态</p>
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              {hasSkills
                ? `已激活 ${enabledSkills.length} 个 Skill`
                : "使用核心默认工具集"}
            </p>
          </div>

          <div className="max-h-64 overflow-y-auto p-2">
            <section>
              <p className="px-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                激活 Skills
              </p>
              {hasSkills ? (
                <ul className="space-y-0.5">
                  {enabledSkills.map((skill) => (
                    <li
                      key={skill.name}
                      className="flex items-center justify-between gap-2 rounded-md px-2 py-1.5 hover:bg-muted/50"
                    >
                      <span className="truncate text-xs font-medium text-foreground">
                        {skill.name}
                      </span>
                      {skill.legacy_trigger ? (
                        <Badge
                          variant="outline"
                          className="h-4 shrink-0 px-1 text-[9px] text-amber-600"
                        >
                          旧格式
                        </Badge>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="px-2 py-1 text-xs text-muted-foreground">
                  暂无激活 Skill
                </p>
              )}
            </section>

            <div className="my-2 border-t border-border/40" />

            <section>
              <p className="px-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                安全策略
              </p>
              <div className="space-y-0.5">
                <PolicyRow
                  icon={Wrench}
                  label="核心工具"
                  detail="始终可用，只读访问"
                />
                <PolicyRow
                  icon={Shield}
                  label="写入工具"
                  detail="执行前需你确认"
                />
                <PolicyRow
                  icon={webSearchEnabled ? Globe : Lock}
                  label="联网搜索"
                  detail={webSearchEnabled ? "已开启" : "未开启"}
                  accent={webSearchEnabled ? "success" : "muted"}
                />
              </div>
            </section>
          </div>

          {(onOpenSkills || (auditAvailable && onOpenAudit)) && (
            <div className="flex items-center gap-2 border-t border-border/60 px-3 py-2">
              {onOpenSkills ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 flex-1 px-2 text-[10px]"
                  onClick={() => {
                    close();
                    onOpenSkills();
                  }}
                >
                  <Puzzle className="mr-1 inline h-3 w-3" />
                  管理 Skills
                </Button>
              ) : null}
              {auditAvailable && onOpenAudit ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 flex-1 px-2 text-[10px]"
                  onClick={() => {
                    close();
                    onOpenAudit();
                  }}
                >
                  工具审计
                </Button>
              ) : null}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
