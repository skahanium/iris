import { Activity, Globe, Lock, Puzzle, Shield, Wrench } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { getActiveAiScene } from "@/hooks/useConnectivityStatus";
import {
  listenSkillsChanged,
  skillsList,
  type SkillListEntryDto,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { AiScene, AssistantTaskStatus, TaskPlanIntent } from "@/types/ai";

interface AgentStatusBadgeProps {
  webSearchEnabled?: boolean;
  disabled?: boolean;
  scene?: AiScene;
  taskPlanIntent?: TaskPlanIntent | null;
  taskStatus?: AssistantTaskStatus;
  onOpenSkills?: () => void;
}

function currentTurnLabel(
  taskPlanIntent: TaskPlanIntent | null | undefined,
  taskStatus: AssistantTaskStatus | undefined,
  fallbackScene: AiScene,
): string {
  if (taskStatus === "awaiting_confirmation") return "等待确认";

  switch (taskPlanIntent) {
    case "creative_write":
    case "rewrite_selection":
    case "chapter":
    case "document_check":
      return "写作候选";
    case "citation_check":
      return "引用核查";
    case "research":
      return "研究综合";
    case "organize":
    case "ask_notes":
    case "chat":
    case "vision_chat":
    case "skill_management":
      return "轻量对话";
    default:
      return fallbackScene === "research_synthesis"
        ? "研究综合"
        : fallbackScene === "drafting_assist"
          ? "写作候选"
          : "轻量对话";
  }
}

function sceneForSkillCompatibility(
  taskPlanIntent: TaskPlanIntent | null | undefined,
  fallbackScene: AiScene,
): AiScene {
  switch (taskPlanIntent) {
    case "creative_write":
    case "rewrite_selection":
    case "chapter":
    case "document_check":
      return "drafting_assist";
    case "citation_check":
    case "research":
      return "research_synthesis";
    case "ask_notes":
    case "chat":
    case "vision_chat":
    case "skill_management":
    case "organize":
      return "knowledge_lookup";
    default:
      return fallbackScene;
  }
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
  scene: sceneProp,
  taskPlanIntent,
  taskStatus,
  onOpenSkills,
}: AgentStatusBadgeProps) {
  const [skills, setSkills] = useState<SkillListEntryDto[]>([]);
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const fallbackScene = sceneProp ?? getActiveAiScene();
  const scene = sceneForSkillCompatibility(taskPlanIntent, fallbackScene);

  const loadSkills = useCallback(async () => {
    try {
      setSkills(await skillsList(scene));
    } catch {
      setSkills([]);
    }
  }, [scene]);

  useEffect(() => {
    if (!open) return;
    void loadSkills();
  }, [open, loadSkills]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listenSkillsChanged(() => {
      void loadSkills();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [loadSkills]);

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
  const activeSkills = enabledSkills.filter((s) => s.task_active === true);
  const hasActiveSkills = activeSkills.length > 0;

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
              本轮：
              {currentTurnLabel(taskPlanIntent, taskStatus, fallbackScene)}
              {hasActiveSkills
                ? ` · 当前可用 ${activeSkills.length} 个 Skill`
                : enabledSkills.length > 0
                  ? ` · ${enabledSkills.length} 个已启用但未注入`
                  : " · 使用核心默认工具集"}
            </p>
          </div>

          <div className="max-h-64 overflow-y-auto p-2">
            <section>
              <p className="px-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                当前可用
              </p>
              {hasActiveSkills ? (
                <ul className="space-y-0.5">
                  {activeSkills.map((skill) => (
                    <li
                      key={`${skill.scope}-${skill.name}`}
                      className="rounded-md px-2 py-1.5 hover:bg-muted/50"
                    >
                      <div className="flex items-center justify-between gap-2">
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
                      </div>
                      {skill.allowed_tools.length > 0 ? (
                        <p className="mt-0.5 text-[10px] text-muted-foreground">
                          工具：{skill.allowed_tools.join(", ")}
                        </p>
                      ) : null}
                      {skill.confirmation_required_tools.length > 0 ? (
                        <p className="mt-0.5 text-[10px] text-amber-600">
                          需确认：{skill.confirmation_required_tools.join(", ")}
                        </p>
                      ) : null}
                      {skill.unrecognized_tools.length > 0 ? (
                        <p className="mt-0.5 text-[10px] text-red-500">
                          未识别：{skill.unrecognized_tools.join(", ")}
                        </p>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="px-2 py-1 text-xs text-muted-foreground">
                  当前无可用 Skill
                  {enabledSkills.length > 0
                    ? `（${enabledSkills.length} 个已启用但未匹配当前任务）`
                    : ""}
                </p>
              )}
            </section>

            {enabledSkills.length > activeSkills.length ? (
              <>
                <div className="my-2 border-t border-border/40" />
                <section>
                  <p className="px-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                    已启用（未注入）
                  </p>
                  <ul className="space-y-0.5">
                    {enabledSkills
                      .filter((s) => s.task_active !== true)
                      .map((skill) => (
                        <li
                          key={`idle-${skill.scope}-${skill.name}`}
                          className="truncate px-2 py-1 text-xs text-muted-foreground"
                        >
                          {skill.name}
                        </li>
                      ))}
                  </ul>
                </section>
              </>
            ) : null}

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

          {onOpenSkills ? (
            <div className="flex items-center gap-2 border-t border-border/60 px-3 py-2">
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
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
