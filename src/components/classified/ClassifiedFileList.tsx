import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  ChevronLeft,
  Download,
  FilePlus,
  FolderPlus,
  Lock,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  isImportableUserNotePath,
  vaultRelativePath,
} from "@/lib/classified-path";
import { invokeErrorMessage } from "@/lib/credentials";
import { normalizeOpenDialogPath } from "@/lib/dialog-path";
import { quoteYamlString } from "@/lib/frontmatter";
import {
  classifiedDelete,
  classifiedExport,
  classifiedFiles,
  classifiedImport,
  classifiedMkdir,
  classifiedRename,
  fileCreate,
  folderList,
  vaultGet,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { ClassifiedFileEntry } from "@/types/ipc";

interface ClassifiedFileListProps {
  idleDeadline: number | null;
  onLock: () => void;
  onOpenFile: (path: string) => void;
  onActivity: () => void;
}

function IdleCountdown({ deadline }: { deadline: number | null }) {
  const [label, setLabel] = useState<string | null>(null);

  useEffect(() => {
    if (!deadline) {
      setLabel(null);
      return;
    }
    const tick = () => {
      const remain = Math.max(0, deadline - Date.now());
      const min = Math.floor(remain / 60_000);
      const sec = Math.floor((remain % 60_000) / 1000);
      setLabel(`${min}:${sec.toString().padStart(2, "0")}`);
    };
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [deadline]);

  if (!label) return null;
  return (
    <span
      className="text-xs text-muted-foreground"
      data-testid="classified-idle-countdown"
    >
      自动锁定 {label}
    </span>
  );
}

function displayName(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] ?? path;
}

function folderKey(folder: string): string | undefined {
  const trimmed = folder
    .replace(/\\/g, "/")
    .trim()
    .replace(/^\.classified\/?/, "");
  return trimmed.length > 0 ? trimmed : undefined;
}

function nextUntitledPath(
  entries: ClassifiedFileEntry[],
  folder: string,
): string {
  const prefix = folder === ".classified" ? ".classified" : folder;
  const taken = new Set(
    entries.filter((e) => !e.isDir).map((e) => displayName(e.path)),
  );
  for (let i = 1; i < 100; i += 1) {
    const name = i === 1 ? "未命名.md" : `未命名 ${i}.md`;
    if (!taken.has(name)) {
      return `${prefix}/${name}`;
    }
  }
  return `${prefix}/未命名.md`;
}

export function ClassifiedFileList({
  idleDeadline,
  onLock,
  onOpenFile,
  onActivity,
}: ClassifiedFileListProps) {
  const [currentFolder, setCurrentFolder] = useState(".classified");
  const [files, setFiles] = useState<ClassifiedFileEntry[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ open: boolean; x: number; y: number }>({
    open: false,
    x: 0,
    y: 0,
  });

  const refresh = useCallback(async () => {
    try {
      const list = await classifiedFiles(folderKey(currentFolder));
      setFiles(list);
      setError(null);
    } catch (e) {
      setError(invokeErrorMessage(e));
    }
  }, [currentFolder]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const breadcrumbs = useMemo(() => {
    const rel = currentFolder.replace(/^\.classified\/?/, "");
    const parts = rel ? rel.split("/").filter(Boolean) : [];
    const crumbs: { label: string; path: string }[] = [
      { label: ".classified", path: ".classified" },
    ];
    let acc = ".classified";
    for (const part of parts) {
      acc = `${acc}/${part}`;
      crumbs.push({ label: part, path: acc });
    }
    return crumbs;
  }, [currentFolder]);

  const runAction = useCallback(
    async (action: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      try {
        await action();
        onActivity();
        await refresh();
      } catch (e) {
        setError(invokeErrorMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [onActivity, refresh],
  );

  const handleCreateNote = () => {
    void runAction(async () => {
      const path = nextUntitledPath(files, currentFolder);
      const title = displayName(path).replace(/\.md$/i, "");
      const content = `---\ntitle: ${quoteYamlString(title)}\n---\n\n`;
      await fileCreate(path, content);
      setSelected(path);
    });
  };

  const handleCreateFolder = () => {
    const name = window.prompt("新建子文件夹名称");
    if (!name?.trim()) return;
    const safe = name.trim().replace(/[/\\]/g, "-");
    const folder =
      currentFolder === ".classified"
        ? `.classified/${safe}`
        : `${currentFolder}/${safe}`;
    void runAction(async () => {
      await classifiedMkdir(folder);
    });
  };

  const handleImport = () => {
    void runAction(async () => {
      const vaultPath = await vaultGet();
      if (!vaultPath) {
        throw new Error("未打开笔记库");
      }
      const selected = await openFileDialog({
        multiple: false,
        defaultPath: vaultPath,
        title: "选择要导入的笔记",
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      const absolute = normalizeOpenDialogPath(selected);
      if (!absolute) return;
      const relative = vaultRelativePath(vaultPath, absolute);
      if (!relative || !isImportableUserNotePath(relative)) {
        throw new Error("只能导入普通用户笔记");
      }
      const targetFolder =
        currentFolder === ".classified" ? ".classified" : currentFolder;
      await classifiedImport(relative, targetFolder);
    });
  };

  const handleExport = (path: string) => {
    void runAction(async () => {
      const folders = await folderList();
      const defaultFolder = folders[0] ?? "";
      const choice = window.prompt(
        `导出到笔记库内的文件夹（例如 notes）\n可用：${folders.slice(0, 8).join(", ") || "根目录"}`,
        defaultFolder.replace(/\/$/, ""),
      );
      if (!choice?.trim()) return;
      const target = choice.trim().replace(/\\/g, "/").replace(/\/$/, "");
      if (
        target.startsWith(".iris") ||
        target.startsWith(".classified") ||
        target.includes("..")
      ) {
        throw new Error("只能导出到普通笔记目录");
      }
      const overwrite = window.confirm(
        `将导出到「${target}」。若目标已有同名文件，操作将失败。是否继续？`,
      );
      if (!overwrite) return;
      await classifiedExport(path, target);
      if (selected === path) {
        setSelected(null);
      }
    });
  };

  const handleDelete = (path: string) => {
    const label = displayName(path);
    if (!window.confirm(`确定删除「${label}」？此操作不可撤销。`)) return;
    void runAction(async () => {
      await classifiedDelete(path);
      if (selected === path) {
        setSelected(null);
      }
    });
  };

  const handleRename = (path: string) => {
    const current = displayName(path);
    const nextName = window.prompt("重命名为", current);
    if (!nextName?.trim() || nextName.trim() === current) return;
    const parent = path.replace(/\\/g, "/").replace(/\/[^/]+$/, "");
    const newPath = `${parent}/${nextName.trim()}`;
    void runAction(async () => {
      await classifiedRename(path, newPath);
      if (selected === path) {
        setSelected(newPath);
      }
    });
  };

  const menuTarget = menu.open ? selected : null;

  return (
    <div
      className="flex min-h-0 flex-1 flex-col gap-2 p-4"
      data-testid="classified-file-list"
      onMouseMove={onActivity}
      onKeyDown={onActivity}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 className="text-lg font-semibold">涉密文件</h3>
          <IdleCountdown deadline={idleDeadline} />
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="gap-1.5"
          onClick={onLock}
          disabled={busy}
        >
          <Lock className="h-3.5 w-3.5" />
          锁定
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
        {currentFolder !== ".classified" ? (
          <button
            type="button"
            className="inline-flex items-center gap-0.5 rounded px-1 py-0.5 hover:bg-muted"
            onClick={() => {
              const parent = currentFolder.replace(/\/[^/]+$/, "");
              setCurrentFolder(parent || ".classified");
              setSelected(null);
              onActivity();
            }}
          >
            <ChevronLeft className="h-3 w-3" />
            上级
          </button>
        ) : null}
        {breadcrumbs.map((crumb) => (
          <button
            key={crumb.path}
            type="button"
            className={cn(
              "rounded px-1 py-0.5 hover:bg-muted",
              crumb.path === currentFolder && "font-medium text-foreground",
            )}
            onClick={() => {
              setCurrentFolder(crumb.path);
              setSelected(null);
              onActivity();
            }}
          >
            {crumb.label}
          </button>
        ))}
      </div>

      {error ? (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      ) : null}

      <ScrollArea className="min-h-[200px] flex-1 rounded-md border border-border/60">
        <div className="p-1">
          {files.map((entry) => (
            <button
              key={entry.path}
              type="button"
              className={cn(
                "flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:bg-muted",
                selected === entry.path && "bg-muted",
              )}
              onClick={() => {
                onActivity();
                if (entry.isDir) {
                  setCurrentFolder(entry.path);
                  setSelected(null);
                  return;
                }
                setSelected(entry.path);
                onOpenFile(entry.path);
              }}
              onContextMenu={(event) => {
                if (entry.isDir) return;
                event.preventDefault();
                setSelected(entry.path);
                setMenu({ open: true, x: event.clientX, y: event.clientY });
                onActivity();
              }}
            >
              <span aria-hidden>{entry.isDir ? "📁" : "📄"}</span>
              <span className="truncate">{displayName(entry.path)}</span>
            </button>
          ))}
          {files.length === 0 ? (
            <p className="px-2 py-4 text-sm text-muted-foreground">
              涉密文件夹为空
            </p>
          ) : null}
        </div>
      </ScrollArea>

      <div className="flex flex-wrap gap-2 border-t border-border/60 pt-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="gap-1.5"
          onClick={handleCreateNote}
          disabled={busy}
        >
          <FilePlus className="h-3.5 w-3.5" />
          新建
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="gap-1.5"
          onClick={handleImport}
          disabled={busy}
        >
          <Upload className="h-3.5 w-3.5" />
          导入
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="gap-1.5"
          onClick={handleCreateFolder}
          disabled={busy}
        >
          <FolderPlus className="h-3.5 w-3.5" />
          新建文件夹
        </Button>
        {selected && !files.find((f) => f.path === selected)?.isDir ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="gap-1.5"
            onClick={() => handleExport(selected)}
            disabled={busy}
          >
            <Download className="h-3.5 w-3.5" />
            导出
          </Button>
        ) : null}
      </div>

      <IrisContextMenu
        open={menu.open}
        x={menu.x}
        y={menu.y}
        groups={[
          {
            group: "file",
            items: [
              {
                id: "open",
                label: "打开",
                icon: "FileText",
                disabled: !menuTarget,
              },
              {
                id: "export",
                label: "导出",
                icon: "Download",
                disabled: !menuTarget,
              },
              {
                id: "rename",
                label: "重命名",
                icon: "Pencil",
                disabled: !menuTarget,
              },
              {
                id: "delete",
                label: "删除",
                icon: "Trash2",
                disabled: !menuTarget,
              },
            ],
          },
        ]}
        onSelect={(id) => {
          if (!menuTarget) return;
          setMenu({ open: false, x: 0, y: 0 });
          if (id === "open") {
            onOpenFile(menuTarget);
          } else if (id === "export") {
            handleExport(menuTarget);
          } else if (id === "rename") {
            handleRename(menuTarget);
          } else if (id === "delete") {
            handleDelete(menuTarget);
          }
        }}
        onClose={() => setMenu({ open: false, x: 0, y: 0 })}
      />
    </div>
  );
}
