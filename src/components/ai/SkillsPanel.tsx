import { Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { getActiveAiScene } from "@/hooks/useConnectivityStatus";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  listenSkillsChanged,
  skillsList,
  type SkillListEntryDto,
} from "@/lib/ipc";
import { SkillCard } from "@/components/ai/skills/SkillCard";

interface SkillsPanelProps {
  open: boolean;
  onClose: () => void;
  scene?: import("@/types/ai").AiScene;
}

type SkillScope = "global" | "vault";

interface CapabilityGroup {
  label: string;
  tone: "calm" | "info" | "warn" | "danger";
}

const SKILL_PERMISSION_SUMMARY_LABEL = "权限摘要";

const CRITICAL_CAPABILITIES = new Set([
  "execute_script_sandboxed",
  "install_dependency",
  "mcp_bridge",
  "bash",
  "shell",
  "computer",
  "computer_control",
]);

function scopeLabel(scope: string): SkillScope {
  return scope === "vault" ? "vault" : "global";
}

function sourceSummary(skill: SkillListEntryDto): string {
  return skill.file_path;
}

function blockedCapabilities(skill: SkillListEntryDto) {
  return (
    skill.blockedCapabilities ??
    skill.capability_preview?.blocked_capabilities ??
    []
  );
}

function hasBlockedCriticalCapabilities(skill: SkillListEntryDto): boolean {
  return blockedCapabilities(skill).some(
    (capability) =>
      capability.status === "blocked_by_policy" ||
      CRITICAL_CAPABILITIES.has(capability.capability.toLowerCase()),
  );
}

function hasCompatibilityWarning(skill: SkillListEntryDto): boolean {
  const warnings =
    skill.compatibilityWarnings ??
    skill.capability_preview?.compatibility_warnings ??
    [];
  return warnings.length > 0 || blockedCapabilities(skill).length > 0;
}

function collectCapabilityTokens(skill: SkillListEntryDto): string[] {
  const blocked =
    skill.blockedCapabilities ??
    skill.capability_preview?.blocked_capabilities ??
    [];
  return [
    ...skill.allowed_tools,
    ...skill.confirmation_required_tools,
    ...blocked.map((capability) => capability.capability),
  ].map((token) => token.toLowerCase());
}

function capabilityGroups(skill: SkillListEntryDto): CapabilityGroup[] {
  const tokens = collectCapabilityTokens(skill);
  const joined = tokens.join(" ");
  const groups: CapabilityGroup[] = [];

  const add = (group: CapabilityGroup) => {
    if (!groups.some((item) => item.label === group.label)) {
      groups.push(group);
    }
  };

  if (
    joined.includes("read") ||
    joined.includes("search") ||
    joined.includes("note") ||
    joined.includes("vault")
  ) {
    add({ label: "只读笔记", tone: "calm" });
  }
  if (
    joined.includes("web") ||
    joined.includes("http") ||
    joined.includes("fetch") ||
    joined.includes("download")
  ) {
    add({ label: "联网读取", tone: "info" });
  }
  if (
    joined.includes("write") ||
    joined.includes("replace") ||
    joined.includes("insert") ||
    joined.includes("edit")
  ) {
    add({ label: "写入笔记", tone: "warn" });
  }
  if (joined.includes("skills_") || joined.includes("skill.")) {
    add({ label: "管理 Skills", tone: "warn" });
  }
  if (
    joined.includes("command") ||
    joined.includes("process") ||
    joined.includes("bash") ||
    joined.includes("shell") ||
    joined.includes("execute_script")
  ) {
    add({ label: "运行命令", tone: "danger" });
  }
  if (
    joined.includes("credential") ||
    joined.includes("secret") ||
    joined.includes("token") ||
    joined.includes("api_key")
  ) {
    add({ label: "凭据访问", tone: "danger" });
  }

  if (groups.length === 0) {
    groups.push({ label: "只读说明", tone: "calm" });
  }
  return groups;
}

function capabilityToneClass(tone: CapabilityGroup["tone"]): string {
  switch (tone) {
    case "info":
      return "border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900/60 dark:bg-sky-950/35 dark:text-sky-300";
    case "warn":
      return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/35 dark:text-amber-300";
    case "danger":
      return "border-red-200 bg-red-50 text-red-700 dark:border-red-900/60 dark:bg-red-950/35 dark:text-red-300";
    case "calm":
    default:
      return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/60 dark:bg-emerald-950/35 dark:text-emerald-300";
  }
}

function confirmationState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
} {
  if (skill.confirmation_status === "confirmed") {
    return {
      label: "已确认",
      detail: "当前 SKILL.md 内容哈希已经由用户确认。",
    };
  }
  return {
    label: "需要确认",
    detail: "内容新增或变更后，需要用户确认后才会参与提示注入。",
  };
}

function sectionState(skill: SkillListEntryDto): {
  activated: string[];
  blocked: string[];
} {
  return {
    activated: skill.activated_sections ?? [],
    blocked: skill.blocked_sections ?? [],
  };
}
export function SkillsPanelBody({
  open,
  scene,
}: {
  open: boolean;
  scene?: import("@/types/ai").AiScene;
}) {
  const [skills, setSkills] = useState<SkillListEntryDto[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  const legacySceneHint = scene ?? getActiveAiScene();

  const refresh = useCallback(async () => {
    try {
      const nextSkills = await skillsList(legacySceneHint);
      setSkills(nextSkills);
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    }
  }, [legacySceneHint]);

  useEffect(() => {
    if (!open) return;
    void refresh();
  }, [open, refresh]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenSkillsChanged(() => {
      if (disposed) return;
      void refresh();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open, refresh]);

  const filtered = useMemo(
    () =>
      skills.filter(
        (skill) =>
          !query.trim() ||
          skill.name.toLowerCase().includes(query.toLowerCase()) ||
          skill.description.toLowerCase().includes(query.toLowerCase()),
      ),
    [query, skills],
  );

  const global = filtered.filter(
    (skill) => scopeLabel(skill.scope) === "global",
  );
  const vault = filtered.filter((skill) => scopeLabel(skill.scope) === "vault");

  const renderSkillCard = (skill: SkillListEntryDto) => {
    const sc = scopeLabel(skill.scope);
    const criticalBlocked = hasBlockedCriticalCapabilities(skill);
    const compatibilityWarning = hasCompatibilityWarning(skill);
    const groups = capabilityGroups(skill);
    const confirmation = confirmationState(skill);
    const sections = sectionState(skill);

    return (
      <SkillCard
        key={`${sc}-${skill.name}`}
        skill={skill}
        sourceSummary={sourceSummary(skill)}
        confirmation={confirmation}
        sections={sections}
        capabilityGroups={groups}
        capabilityToneClass={capabilityToneClass}
        capabilitySummaryLabel={SKILL_PERMISSION_SUMMARY_LABEL}
        criticalBlocked={criticalBlocked}
        compatibilityWarning={compatibilityWarning}
        onUpdate={() => void refresh()}
      />
    );
  };

  const renderGroup = (title: string, items: SkillListEntryDto[]) => (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground">{title}</p>
        <span className="text-[10px] text-muted-foreground">
          {items.length}
        </span>
      </div>
      {items.length === 0 ? (
        <p className="rounded-md border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
          暂无 Skills
        </p>
      ) : (
        items.map(renderSkillCard)
      )}
    </div>
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="skills-panel">
      <div className="task-overlay-filter flex shrink-0 items-center justify-between border-b border-border/60 px-3 py-2">
        <p className="text-xs font-medium text-muted-foreground">Skills</p>
      </div>

      <ScrollArea className="task-overlay-results flex-1">
        <div className="space-y-3 p-3">
          <div className="relative">
            <Search className="absolute left-2 top-2 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              className="h-8 pl-8 text-xs"
              placeholder="搜索 Skills"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          {renderGroup("当前库", vault)}
          {renderGroup("全局", global)}
        </div>
      </ScrollArea>
    </div>
  );
}

export function SkillsPanel({ open, onClose, scene }: SkillsPanelProps) {
  return (
    <IrisOverlay open={open} onClose={onClose} title="AI Skills" size="command">
      <SkillsPanelBody open={open} scene={scene} />
    </IrisOverlay>
  );
}
