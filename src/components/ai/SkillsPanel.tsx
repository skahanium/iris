import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Download, Search } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type DragEvent,
} from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { getActiveAiScene } from "@/hooks/useConnectivityStatus";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  listenSkillsChanged,
  skillsInstall,
  skillsList,
  skillsMigrateLegacy,
  skillsPaths,
  skillsPrepareWorkspace,
  skillsRead,
  skillsToggle,
  skillsUninstall,
  skillsUpdate,
  skillsWrite,
  type SkillListEntryDto,
  type SkillsPathsDto,
} from "@/lib/ipc";
import { McpProfilesPanel } from "@/components/ai/skills/McpProfilesPanel";
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

const CRITICAL_CAPABILITIES = new Set([
  "skill.execute_script_sandboxed",
  "skill.install_dependency",
  "skill.mcp_bridge",
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
  if (skill.source_url) return skill.source_url;
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
  const requested =
    skill.requestedCapabilities ??
    skill.capability_preview?.requested_capabilities ??
    [];

  return [
    ...skill.allowed_tools,
    ...skill.confirmation_required_tools,
    ...requested,
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
    joined.includes("vault") ||
    joined.includes("skill.read_resource")
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
    joined.includes("edit") ||
    joined.includes("skill.write_storage")
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

function workspaceState(skill: SkillListEntryDto): {
  label: string;
  detail: string;
  needsPrepare: boolean;
} {
  const root =
    skill.workspace_root ?? skill.workspaceRoot ?? `Skills/${skill.name}`;
  const missing =
    skill.workspace_missing_items ?? skill.workspaceMissingItems ?? [];
  const declared = skill.workspace_declared ?? true;
  const prepared =
    skill.workspace_prepared ?? skill.workspace_ready ?? skill.workspaceReady;

  if (!declared) {
    return {
      label: "无工作区",
      detail: "该 Skill 不声明独立工作区。",
      needsPrepare: false,
    };
  }

  if (prepared === false) {
    const summary =
      missing.length > 0
        ? `缺少 ${missing.slice(0, 3).join("、")}${missing.length > 3 ? " 等" : ""}`
        : "尚未准备";
    return {
      label: "需要准备",
      detail: `${root} · ${summary}`,
      needsPrepare: true,
    };
  }

  return {
    label: "已准备",
    detail: root,
    needsPrepare: false,
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
  const [paths, setPaths] = useState<SkillsPathsDto | null>(null);
  const [query, setQuery] = useState("");
  const [url, setUrl] = useState("");
  const [gitUrl, setGitUrl] = useState("");
  const [gitSubpath, setGitSubpath] = useState("");
  const [scope, setScope] = useState<"global" | "vault">("vault");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [showInstall, setShowInstall] = useState(false);
  const [editingSkill, setEditingSkill] = useState<SkillListEntryDto | null>(
    null,
  );
  const [editContent, setEditContent] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [pendingWorkspaceSkill, setPendingWorkspaceSkill] = useState<
    string | null
  >(null);
  const [activeTab, setActiveTab] = useState<"skills" | "mcp">("skills");

  const legacySceneHint = scene ?? getActiveAiScene();
  const installTargetPath =
    paths?.[scope] ??
    (scope === "vault" ? "<当前库>/.iris/skills" : "<用户目录>/.iris/skills");

  const refresh = useCallback(async () => {
    try {
      const [nextSkills, nextPaths] = await Promise.all([
        skillsList(legacySceneHint),
        skillsPaths(),
      ]);
      setSkills(nextSkills);
      setPaths(nextPaths);
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

  const installUrl = async () => {
    if (!url.trim()) return;
    setLoading(true);
    setError(null);
    try {
      await skillsInstall({
        source: "url",
        path_or_url: url.trim(),
        scope,
      });
      setUrl("");
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setLoading(false);
    }
  };

  const installLocalPath = async (filePath: string) => {
    setLoading(true);
    setError(null);
    try {
      await skillsInstall({
        source: "local",
        path_or_url: filePath,
        scope,
      });
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setLoading(false);
    }
  };

  const pickLocalFile = async () => {
    const selected = await openFileDialog({
      multiple: false,
      filters: [{ name: "Skill", extensions: ["md"] }],
    });
    if (typeof selected === "string") {
      await installLocalPath(selected);
    }
  };

  const onDropFiles = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragOver(false);
    const file = event.dataTransfer.files[0];
    if (!file?.name.toLowerCase().endsWith(".md")) {
      setError("请拖入 SKILL.md 文件。");
      return;
    }
    const path = (file as File & { path?: string }).path;
    if (path) {
      await installLocalPath(path);
    } else {
      setError("无法读取拖入文件路径，请改用“选择本地文件”。");
    }
  };

  const openEditor = async (skill: SkillListEntryDto) => {
    setError(null);
    try {
      const content = await skillsRead(skill.file_path);
      setEditingSkill(skill);
      setEditContent(content);
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    }
  };

  const saveEditor = async () => {
    if (!editingSkill) return;
    setLoading(true);
    setError(null);
    try {
      await skillsWrite(
        editingSkill.file_path,
        scopeLabel(editingSkill.scope),
        editContent,
      );
      setEditingSkill(null);
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setLoading(false);
    }
  };

  const installGit = async () => {
    if (!gitUrl.trim()) return;
    setLoading(true);
    setError(null);
    try {
      await skillsInstall({
        source: "git",
        path_or_url: gitUrl.trim(),
        scope,
        subpath: gitSubpath.trim() || undefined,
      });
      setGitUrl("");
      setGitSubpath("");
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setLoading(false);
    }
  };

  const prepareWorkspace = async (skill: SkillListEntryDto) => {
    const sc = scopeLabel(skill.scope);
    setPendingWorkspaceSkill(`${sc}:${skill.name}`);
    setError(null);
    try {
      await skillsPrepareWorkspace(skill.name, sc);
      await refresh();
    } catch (nextError) {
      setError(invokeErrorMessage(nextError));
    } finally {
      setPendingWorkspaceSkill(null);
    }
  };

  const renderSkillCard = (skill: SkillListEntryDto) => {
    const sc = scopeLabel(skill.scope);
    const criticalBlocked = hasBlockedCriticalCapabilities(skill);
    const compatibilityWarning = hasCompatibilityWarning(skill);
    const groups = capabilityGroups(skill);
    const workspace = workspaceState(skill);
    const sections = sectionState(skill);
    const workspacePending = pendingWorkspaceSkill === `${sc}:${skill.name}`;

    return (
      <SkillCard
        key={`${sc}-${skill.name}`}
        skill={skill}
        sourceSummary={sourceSummary(skill)}
        workspace={workspace}
        sections={sections}
        capabilityGroups={groups}
        capabilityToneClass={capabilityToneClass}
        criticalBlocked={criticalBlocked}
        compatibilityWarning={compatibilityWarning}
        workspacePending={workspacePending}
        onPrepareWorkspace={() => void prepareWorkspace(skill)}
        onUpdate={() => void skillsUpdate(skill.name, sc).then(refresh)}
        onEdit={() => void openEditor(skill)}
        onMigrate={() => {
          if (
            confirm(
              `将 “${skill.name}” 迁移到新格式？\n\n原文件会备份为 SKILL.md.bak`,
            )
          ) {
            void skillsMigrateLegacy(skill.file_path, sc).then(refresh);
          }
        }}
        onToggle={(enabled) =>
          void skillsToggle(skill.name, sc, enabled).then(refresh)
        }
        onUninstall={() => void skillsUninstall(skill.name, sc).then(refresh)}
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
        <div className="flex items-center gap-1 rounded-md border border-border/70 bg-muted/35 p-0.5">
          <Button
            type="button"
            variant={activeTab === "skills" ? "secondary" : "ghost"}
            size="sm"
            className="h-7 text-xs"
            onClick={() => setActiveTab("skills")}
          >
            Skills
          </Button>
          <Button
            type="button"
            variant={activeTab === "mcp" ? "secondary" : "ghost"}
            size="sm"
            className="h-7 text-xs"
            onClick={() => setActiveTab("mcp")}
          >
            MCP / Providers
          </Button>
        </div>
        {activeTab === "skills" ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() => setShowInstall((value) => !value)}
          >
            {showInstall ? "收起安装" : "安装 Skill"}
          </Button>
        ) : null}
      </div>

      <ScrollArea className="task-overlay-results flex-1">
        <div
          className={`space-y-3 p-3 ${
            dragOver ? "ring-2 ring-inset ring-primary/40" : ""
          }`}
          onDragOver={(event) => {
            event.preventDefault();
            setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={(event) => void onDropFiles(event)}
        >
          {activeTab === "mcp" ? (
            <McpProfilesPanel open={open && activeTab === "mcp"} />
          ) : null}

          {activeTab === "skills" && editingSkill ? (
            <div className="space-y-2 rounded-lg border border-border/70 bg-background p-3 shadow-sm">
              <p className="text-xs font-medium">编辑 {editingSkill.name}</p>
              <Textarea
                className="min-h-[220px] font-mono text-xs"
                value={editContent}
                onChange={(event) => setEditContent(event.target.value)}
              />
              <div className="flex gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={loading}
                  onClick={() => void saveEditor()}
                >
                  保存
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={() => setEditingSkill(null)}
                >
                  取消
                </Button>
              </div>
            </div>
          ) : null}

          {activeTab === "skills" && showInstall ? (
            <div className="space-y-3 rounded-lg border border-border/70 bg-background p-3 shadow-sm">
              <div className="grid gap-2">
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">安装到</span>
                  <select
                    className="h-8 rounded-md border border-border bg-background px-2 text-xs"
                    value={scope}
                    onChange={(event) =>
                      setScope(
                        event.target.value === "global" ? "global" : "vault",
                      )
                    }
                  >
                    <option value="vault">当前库</option>
                    <option value="global">全局</option>
                  </select>
                </div>
                <div className="rounded-md border border-border/70 bg-muted/35 px-2.5 py-2 text-[11px] text-muted-foreground">
                  <span className="font-medium text-foreground/80">
                    目标路径
                  </span>
                  <span className="ml-2 break-all">{installTargetPath}</span>
                </div>
              </div>

              <div className="space-y-2">
                <span className="text-xs text-muted-foreground">网页地址</span>
                <div className="flex gap-2">
                  <Input
                    className="h-8 text-xs"
                    value={url}
                    onChange={(event) => setUrl(event.target.value)}
                    placeholder="https://.../SKILL.md"
                  />
                  <Button
                    type="button"
                    size="sm"
                    disabled={loading}
                    title="从 URL 安装"
                    onClick={() => void installUrl()}
                  >
                    <Download className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>

              <div className="space-y-2">
                <span className="text-xs text-muted-foreground">Git 仓库</span>
                <Input
                  className="h-8 text-xs"
                  value={gitUrl}
                  onChange={(event) => setGitUrl(event.target.value)}
                  placeholder="https://github.com/owner/repo"
                />
                <Input
                  className="h-8 text-xs"
                  value={gitSubpath}
                  onChange={(event) => setGitSubpath(event.target.value)}
                  placeholder="子路径，可选"
                />
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  disabled={loading}
                  onClick={() => void installGit()}
                >
                  从 Git 安装
                </Button>
              </div>

              <div className="space-y-2">
                <span className="text-xs text-muted-foreground">
                  本地 SKILL.md，也可以直接拖进这个面板
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={loading}
                  onClick={() => void pickLocalFile()}
                >
                  选择本地文件
                </Button>
              </div>
            </div>
          ) : null}

          {activeTab === "skills" ? (
            <div className="relative">
              <Search className="absolute left-2 top-2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                className="h-8 pl-8 text-xs"
                placeholder="搜索 Skills"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
          ) : null}

          {error ? <p className="text-xs text-destructive">{error}</p> : null}

          {activeTab === "skills" ? (
            <>
              {renderGroup("当前库", vault)}
              {renderGroup("全局", global)}
            </>
          ) : null}
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
