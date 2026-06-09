import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  Download,
  Pencil,
  Search,
  Trash2,
  ArrowUpCircle,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState, type DragEvent } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { getActiveAiScene } from "@/hooks/useConnectivityStatus";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  skillsInstall,
  skillsList,
  listenSkillsChanged,
  skillsMigrateLegacy,
  skillsRead,
  skillsToggle,
  skillsUninstall,
  skillsUpdate,
  skillsWrite,
  type SkillListEntryDto,
} from "@/lib/ipc";

interface SkillsPanelProps {
  open: boolean;
  onClose: () => void;
  /** When set, list entries include scene_active / scene_score. */
  scene?: import("@/types/ai").AiScene;
}

function scopeLabel(scope: string): "global" | "vault" {
  return scope === "vault" ? "vault" : "global";
}

export function SkillsPanel({
  open: overlayOpen,
  onClose,
  scene,
}: SkillsPanelProps) {
  const [skills, setSkills] = useState<SkillListEntryDto[]>([]);
  const [query, setQuery] = useState("");
  const [url, setUrl] = useState("");
  const [gitUrl, setGitUrl] = useState("");
  const [gitSubpath, setGitSubpath] = useState("");
  const [scope, setScope] = useState<"global" | "vault">("global");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [showInstall, setShowInstall] = useState(false);
  const [editingSkill, setEditingSkill] = useState<SkillListEntryDto | null>(
    null,
  );
  const [editContent, setEditContent] = useState("");
  const [dragOver, setDragOver] = useState(false);

  const activeScene = scene ?? getActiveAiScene();

  const refresh = useCallback(async () => {
    try {
      setSkills(await skillsList(activeScene));
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, [activeScene]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listenSkillsChanged(() => {
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [refresh]);

  const filtered = skills.filter(
    (s) =>
      !query.trim() ||
      s.name.toLowerCase().includes(query.toLowerCase()) ||
      s.description.toLowerCase().includes(query.toLowerCase()),
  );

  const global = filtered.filter((s) => scopeLabel(s.scope) === "global");
  const vault = filtered.filter((s) => scopeLabel(s.scope) === "vault");

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
    } catch (e) {
      setError(invokeErrorMessage(e));
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
    } catch (e) {
      setError(invokeErrorMessage(e));
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
      setError("请拖入 SKILL.md 文件");
      return;
    }
    const path = (file as File & { path?: string }).path;
    if (path) {
      await installLocalPath(path);
    } else {
      setError("无法读取拖放文件路径，请使用「选择本地文件」");
    }
  };

  const openEditor = async (skill: SkillListEntryDto) => {
    setError(null);
    try {
      const content = await skillsRead(skill.file_path);
      setEditingSkill(skill);
      setEditContent(content);
    } catch (e) {
      setError(invokeErrorMessage(e));
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
    } catch (e) {
      setError(invokeErrorMessage(e));
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
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  const renderGroup = (title: string, items: SkillListEntryDto[]) => (
    <div className="space-y-2">
      <p className="text-xs font-medium text-muted-foreground">{title}</p>
      {items.length === 0 ? (
        <p className="text-xs text-muted-foreground">暂无</p>
      ) : (
        items.map((skill) => {
          const sc = scopeLabel(skill.scope);
          const hasLegacy = !!skill.legacy_trigger;
          const hasTools = skill.allowed_tools.length > 0;
          const isInvalid =
            typeof skill.validation === "object" &&
            "invalid" in skill.validation;
          return (
            <div
              key={`${sc}-${skill.name}`}
              className="flex items-start gap-2 rounded-md border border-border/60 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium">{skill.name}</p>
                  {!skill.enabled ? (
                    <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      已禁用
                    </span>
                  ) : skill.scene_active === true ? (
                    <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                      本场景注入
                    </span>
                  ) : skill.scene_active === false ? (
                    <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      已启用
                    </span>
                  ) : null}
                  {hasLegacy ? (
                    <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-600">
                      旧格式
                    </span>
                  ) : null}
                  {isInvalid ? (
                    <span className="rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-600">
                      无效
                    </span>
                  ) : null}
                </div>
                {skill.description ? (
                  <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
                    {skill.description}
                  </p>
                ) : null}
                {skill.legacy_trigger ? (
                  <p className="mt-0.5 text-[10px] text-muted-foreground/80">
                    触发：{skill.legacy_trigger}
                  </p>
                ) : null}
                {hasTools ? (
                  <p className="mt-0.5 text-[10px] text-muted-foreground/80">
                    工具：{skill.allowed_tools.join(", ")}
                  </p>
                ) : null}
                {skill.confirmation_required_tools.length > 0 ? (
                  <p className="mt-0.5 text-[10px] text-amber-600">
                    需确认工具：{skill.confirmation_required_tools.join(", ")}
                  </p>
                ) : null}
                {skill.unrecognized_tools.length > 0 ? (
                  <p className="mt-0.5 text-[10px] text-red-500">
                    未识别工具：{skill.unrecognized_tools.join(", ")}
                  </p>
                ) : null}
                {skill.missing_deps.length > 0 ? (
                  <p className="mt-0.5 text-[10px] text-amber-500">
                    缺失依赖：{skill.missing_deps.join(", ")}
                  </p>
                ) : null}
                {skill.license ? (
                  <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                    许可：{skill.license}
                  </p>
                ) : null}
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                  能力状态：{skill.availability}
                  {skill.content_hash
                    ? ` · ${skill.content_hash.slice(0, 8)}`
                    : ""}
                </p>
                {skill.capability_preview?.script_policy ? (
                  <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                    {String(skill.capability_preview.script_policy)}
                  </p>
                ) : null}
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                title="更新"
                onClick={() => void skillsUpdate(skill.name, sc).then(refresh)}
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                title="编辑 SKILL.md"
                onClick={() => void openEditor(skill)}
              >
                <Pencil className="h-3.5 w-3.5" />
              </Button>
              {hasLegacy ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-amber-600"
                  title="迁移到新格式（会创建 .bak 备份）"
                  onClick={() => {
                    if (
                      confirm(
                        `将 "${skill.name}" 迁移到新格式？\n\n原文件会备份为 SKILL.md.bak`,
                      )
                    ) {
                      void skillsMigrateLegacy(skill.file_path, sc).then(
                        refresh,
                      );
                    }
                  }}
                >
                  <ArrowUpCircle className="h-3.5 w-3.5" />
                </Button>
              ) : null}
              <input
                type="checkbox"
                className="mt-1 h-3.5 w-3.5"
                checked={skill.enabled}
                onChange={(e) => {
                  void skillsToggle(skill.name, sc, e.target.checked).then(
                    refresh,
                  );
                }}
                onClick={(e) => e.stopPropagation()}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-destructive"
                onClick={() => {
                  void skillsUninstall(skill.name, sc).then(refresh);
                }}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          );
        })
      )}
    </div>
  );

  return (
    <IrisOverlay
      open={overlayOpen}
      onClose={onClose}
      title="AI Skills"
      size="command"
    >
      <div className="flex min-h-0 flex-1 flex-col" data-testid="skills-panel">
        <div className="task-overlay-filter flex shrink-0 items-center justify-end border-b border-border/60 px-3 py-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            onClick={() => setShowInstall((v) => !v)}
          >
            {showInstall ? "收起安装" : "安装 Skill"}
          </Button>
        </div>

        <ScrollArea className="task-overlay-results flex-1">
          <div
            className={`space-y-3 p-3 ${
              dragOver ? "ring-2 ring-inset ring-primary/40" : ""
            }`}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(e) => void onDropFiles(e)}
          >
            {editingSkill ? (
              <div className="space-y-2 rounded-md border border-border/60 p-3">
                <p className="text-xs font-medium">编辑 {editingSkill.name}</p>
                <Textarea
                  className="min-h-[200px] font-mono text-xs"
                  value={editContent}
                  onChange={(e) => setEditContent(e.target.value)}
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

            {showInstall ? (
              <div className="space-y-3 rounded-md border border-border/60 p-3">
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">安装到</span>
                  <select
                    className="h-8 rounded-md border border-border bg-background px-2 text-xs"
                    value={scope}
                    onChange={(e) =>
                      setScope(e.target.value === "vault" ? "vault" : "global")
                    }
                  >
                    <option value="global">全局</option>
                    <option value="vault">当前库</option>
                  </select>
                </div>
                <div className="space-y-2">
                  <span className="text-xs text-muted-foreground">URL</span>
                  <div className="flex gap-2">
                    <Input
                      className="h-8 text-xs"
                      value={url}
                      onChange={(e) => setUrl(e.target.value)}
                      placeholder="https://…/SKILL.md"
                    />
                    <Button
                      type="button"
                      size="sm"
                      disabled={loading}
                      onClick={() => void installUrl()}
                    >
                      <Download className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
                <div className="space-y-2">
                  <span className="text-xs text-muted-foreground">
                    Git 仓库
                  </span>
                  <Input
                    className="h-8 text-xs"
                    value={gitUrl}
                    onChange={(e) => setGitUrl(e.target.value)}
                  />
                  <Input
                    className="h-8 text-xs"
                    value={gitSubpath}
                    onChange={(e) => setGitSubpath(e.target.value)}
                    placeholder="子路径（可选）"
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
                    本地 SKILL.md（或拖放到面板）
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

            <div className="relative">
              <Search className="absolute left-2 top-2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                className="h-8 pl-8 text-xs"
                placeholder="搜索 skills…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>

            {error ? <p className="text-xs text-destructive">{error}</p> : null}

            {renderGroup("全局", global)}
            {renderGroup("当前库", vault)}
          </div>
        </ScrollArea>
      </div>
    </IrisOverlay>
  );
}
