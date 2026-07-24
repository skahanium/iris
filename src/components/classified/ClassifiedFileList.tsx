import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ChevronLeft,
  Download,
  FilePlus,
  FileText,
  Folder,
  FolderPlus,
  Lock,
  Pencil,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IrisContextMenu } from "@/components/ui/iris-context-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  classifiedBreadcrumbs,
  classifiedDisplayName,
  isImportableUserNotePath,
  vaultRelativePath,
} from "@/lib/classified-path";
import { invokeErrorMessage } from "@/lib/credentials";
import { normalizeOpenDialogPath } from "@/lib/dialog-path";
import {
  classifiedDelete,
  classifiedExport,
  classifiedFiles,
  classifiedImport,
  classifiedMkdir,
  classifiedRename,
  fileCreate,
  fileRead,
  folderList,
  vaultGet,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { ClassifiedFileEntry } from "@/types/ipc";

interface ClassifiedFileListProps {
  idleDeadline: number | null;
  onLock: () => void;
  onOpenFile: (path: string) => void | Promise<void>;
  onPrepareFile?: (path: string, titleHint?: string) => void;
  onActivity: () => void;
}

type ActionDialog =
  | { type: "createFolder"; name: string }
  | { type: "rename"; path: string; name: string }
  | { type: "delete"; path: string }
  | { type: "export"; path: string; target: string; overwrite: boolean };

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

  if (!label) return <span>闲置后自动锁定</span>;
  return (
    <span data-testid="classified-idle-countdown">
      闲置后自动锁定 · {label}
    </span>
  );
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
    entries.filter((e) => !e.isDir).map((e) => classifiedDisplayName(e.path)),
  );
  for (let i = 1; i < 100; i += 1) {
    const name = i === 1 ? "未命名.md" : `未命名 ${i}.md`;
    if (!taken.has(name)) {
      return `${prefix}/${name}`;
    }
  }
  return `${prefix}/未命名.md`;
}

function sanitizeFolderName(name: string): string {
  return name.trim().replace(/[/\\]/g, "-");
}

function sanitizeExportTarget(target: string): string {
  return target.trim().replace(/\\/g, "/").replace(/\/$/, "");
}

function ActionDialogPanel({
  dialog,
  error,
  busy,
  exportFolders,
  onChange,
  onCancel,
  onConfirm,
}: {
  dialog: ActionDialog;
  error: string | null;
  busy: boolean;
  exportFolders: string[];
  onChange: (dialog: ActionDialog) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const isDanger = dialog.type === "delete";
  const title =
    dialog.type === "createFolder"
      ? "新建文件夹"
      : dialog.type === "rename"
        ? "重命名"
        : dialog.type === "delete"
          ? "删除涉密文件"
          : dialog.overwrite
            ? "确认覆盖导出"
            : "导出涉密文件";

  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/70 p-4 backdrop-blur-sm">
      <div
        role="dialog"
        aria-label={title}
        className="w-full max-w-sm rounded-xl border border-border bg-panel p-4 shadow-overlay"
      >
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            {isDanger ? (
              <AlertTriangle className="h-4 w-4 shrink-0 text-destructive" />
            ) : null}
            <h3 className="truncate text-sm font-semibold">{title}</h3>
          </div>
          <button
            type="button"
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-inset hover:text-foreground"
            onClick={onCancel}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {dialog.type === "createFolder" ? (
          <label className="grid gap-2 text-sm">
            <span className="text-muted-foreground">文件夹名称</span>
            <Input
              value={dialog.name}
              onChange={(event) =>
                onChange({ ...dialog, name: event.target.value })
              }
              onKeyDown={(event) => {
                if (event.key === "Enter") onConfirm();
              }}
              autoFocus
            />
          </label>
        ) : null}

        {dialog.type === "rename" ? (
          <label className="grid gap-2 text-sm">
            <span className="text-muted-foreground">新名称</span>
            <Input
              value={dialog.name}
              onChange={(event) =>
                onChange({ ...dialog, name: event.target.value })
              }
              onKeyDown={(event) => {
                if (event.key === "Enter") onConfirm();
              }}
              autoFocus
            />
          </label>
        ) : null}

        {dialog.type === "delete" ? (
          <p className="text-sm text-muted-foreground">
            将删除「{classifiedDisplayName(dialog.path)}」。此操作不可撤销。
          </p>
        ) : null}

        {dialog.type === "export" ? (
          dialog.overwrite ? (
            <p className="text-sm text-muted-foreground">
              普通笔记目录中已存在「{classifiedDisplayName(dialog.path)}」。
              确认覆盖后会导出为明文笔记。
            </p>
          ) : (
            <div className="grid gap-2 text-sm">
              <label className="grid gap-2">
                <span className="text-muted-foreground">
                  导出到普通笔记目录
                </span>
                <Input
                  value={dialog.target}
                  placeholder="例如 notes"
                  onChange={(event) =>
                    onChange({ ...dialog, target: event.target.value })
                  }
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onConfirm();
                  }}
                  autoFocus
                />
              </label>
              {exportFolders.length > 0 ? (
                <div className="flex flex-wrap gap-1">
                  {exportFolders.slice(0, 6).map((folder) => (
                    <button
                      key={folder || "root"}
                      type="button"
                      className="rounded-md border border-border/60 px-2 py-1 text-xs text-muted-foreground hover:bg-surface-inset hover:text-foreground"
                      onClick={() =>
                        onChange({
                          ...dialog,
                          target: folder.replace(/\/$/, ""),
                        })
                      }
                    >
                      {folder || "根目录"}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          )
        ) : null}

        {error ? (
          <p className="mt-3 text-sm text-destructive" role="alert">
            {error}
          </p>
        ) : null}

        <div className="mt-4 flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            取消
          </Button>
          <Button
            type="button"
            variant={isDanger ? "destructive" : "default"}
            size="sm"
            onClick={onConfirm}
            disabled={busy}
          >
            {dialog.type === "delete"
              ? "删除"
              : dialog.type === "export"
                ? dialog.overwrite
                  ? "覆盖并导出"
                  : "导出"
                : "确认"}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function ClassifiedFileList({
  idleDeadline,
  onLock,
  onOpenFile,
  onPrepareFile,
  onActivity,
}: ClassifiedFileListProps) {
  const [currentFolder, setCurrentFolder] = useState(".classified");
  const [files, setFiles] = useState<ClassifiedFileEntry[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dialog, setDialog] = useState<ActionDialog | null>(null);
  const [exportFolders, setExportFolders] = useState<string[]>([]);
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

  const breadcrumbs = useMemo(
    () => classifiedBreadcrumbs(currentFolder),
    [currentFolder],
  );

  const selectedEntry = files.find((entry) => entry.path === selected);
  const menuTarget = menu.open ? selected : null;

  const runAction = useCallback(
    async (action: () => Promise<void>) => {
      setBusy(true);
      setError(null);
      setDialogError(null);
      try {
        await action();
        onActivity();
        await refresh();
      } catch (e) {
        const message = invokeErrorMessage(e);
        if (dialog) {
          setDialogError(message);
        } else {
          setError(message);
        }
      } finally {
        setBusy(false);
      }
    },
    [dialog, onActivity, refresh],
  );

  const handleCreateNote = () => {
    void runAction(async () => {
      const path = nextUntitledPath(files, currentFolder);
      await fileCreate(path, "");
      setSelected(path);
    });
  };

  const openCreateFolderDialog = () => {
    setDialogError(null);
    setDialog({ type: "createFolder", name: "" });
  };

  const openRenameDialog = (path: string) => {
    setDialogError(null);
    setDialog({
      type: "rename",
      path,
      name: classifiedDisplayName(path),
    });
  };

  const openDeleteDialog = (path: string) => {
    setDialogError(null);
    setDialog({ type: "delete", path });
  };

  const openExportDialog = (path: string) => {
    setDialogError(null);
    setDialog({ type: "export", path, target: "", overwrite: false });
    void folderList()
      .then((folders) =>
        setExportFolders(folders.map((f) => f.replace(/\/$/, ""))),
      )
      .catch(() => setExportFolders([]));
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
      await classifiedImport(relative, currentFolder);
    });
  };

  const submitDialog = () => {
    if (!dialog) return;
    void runAction(async () => {
      if (dialog.type === "createFolder") {
        const safe = sanitizeFolderName(dialog.name);
        if (!safe) throw new Error("请输入文件夹名称");
        const folder =
          currentFolder === ".classified"
            ? `.classified/${safe}`
            : `${currentFolder}/${safe}`;
        await classifiedMkdir(folder);
        setDialog(null);
        return;
      }

      if (dialog.type === "rename") {
        const nextName = dialog.name.trim();
        if (!nextName) throw new Error("请输入新名称");
        const current = classifiedDisplayName(dialog.path);
        if (nextName === current) {
          setDialog(null);
          return;
        }
        const parent = dialog.path.replace(/\\/g, "/").replace(/\/[^/]+$/, "");
        const newPath = `${parent}/${nextName}`;
        await classifiedRename(dialog.path, newPath);
        if (selected === dialog.path) {
          setSelected(newPath);
        }
        setDialog(null);
        return;
      }

      if (dialog.type === "delete") {
        await classifiedDelete(dialog.path);
        if (selected === dialog.path) {
          setSelected(null);
        }
        setDialog(null);
        return;
      }

      const target = sanitizeExportTarget(dialog.target);
      if (!target) throw new Error("请输入普通笔记目录");
      if (
        target.startsWith(".iris") ||
        target.startsWith(".classified") ||
        target.includes("..")
      ) {
        throw new Error("只能导出到普通笔记目录");
      }
      const destPath = `${target}/${classifiedDisplayName(dialog.path)}`;
      if (!dialog.overwrite) {
        try {
          await fileRead(destPath);
          setDialog({ ...dialog, target, overwrite: true });
          return;
        } catch {
          // Missing target is expected; export can continue.
        }
      }
      await classifiedExport(dialog.path, target, dialog.overwrite);
      if (selected === dialog.path) {
        setSelected(null);
      }
      setDialog(null);
    });
  };

  return (
    <div
      className="relative flex min-h-0 flex-1 flex-col gap-3 p-4"
      data-testid="classified-file-list"
      onMouseMove={onActivity}
      onKeyDown={onActivity}
    >
      <div className="flex items-center justify-between gap-3 rounded-lg border border-border/60 bg-surface-inset/40 px-3 py-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-sm font-medium">
            <span className="inline-flex h-2 w-2 rounded-full bg-success" />
            已解锁
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            <IdleCountdown deadline={idleDeadline} />
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="shrink-0 gap-1.5"
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
            className="inline-flex items-center gap-0.5 rounded-md px-1.5 py-1 hover:bg-surface-inset hover:text-foreground"
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
              "rounded-md px-1.5 py-1 hover:bg-surface-inset hover:text-foreground",
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

      <ScrollArea className="min-h-[240px] flex-1 rounded-lg border border-border/60 bg-background/40">
        <div className="p-1.5">
          {files.map((entry) => {
            const Icon = entry.isDir ? Folder : FileText;
            return (
              <button
                key={entry.path}
                type="button"
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm hover:bg-surface-inset",
                  selected === entry.path && "bg-surface-inset text-foreground",
                )}
                onMouseEnter={() =>
                  !entry.isDir &&
                  onPrepareFile?.(entry.path, classifiedDisplayName(entry.path))
                }
                onFocus={() =>
                  !entry.isDir &&
                  onPrepareFile?.(entry.path, classifiedDisplayName(entry.path))
                }
                onClick={() => {
                  onActivity();
                  if (entry.isDir) {
                    setCurrentFolder(entry.path);
                    setSelected(null);
                    return;
                  }
                  setSelected(entry.path);
                  void onOpenFile(entry.path);
                }}
                onContextMenu={(event) => {
                  if (entry.isDir) return;
                  event.preventDefault();
                  setSelected(entry.path);
                  setMenu({ open: true, x: event.clientX, y: event.clientY });
                  onActivity();
                }}
              >
                <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="truncate">
                  {classifiedDisplayName(entry.path)}
                </span>
              </button>
            );
          })}
          {files.length === 0 ? (
            <div className="flex min-h-[220px] flex-col items-center justify-center gap-3 px-4 text-center">
              <div className="flex h-10 w-10 items-center justify-center rounded-full border border-border/60 bg-surface-inset text-muted-foreground">
                <Folder className="h-5 w-5" />
              </div>
              <div>
                <p className="text-sm font-medium">还没有涉密文件</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  新建笔记或导入普通笔记到保险库。
                </p>
              </div>
            </div>
          ) : null}
        </div>
      </ScrollArea>

      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3">
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
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
            onClick={openCreateFolderDialog}
            disabled={busy}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            新建文件夹
          </Button>
        </div>
        {selected && !selectedEntry?.isDir ? (
          <div className="flex gap-1">
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="h-8 w-8"
              aria-label="导出"
              onClick={() => openExportDialog(selected)}
              disabled={busy}
            >
              <Download className="h-4 w-4" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="h-8 w-8"
              aria-label="重命名"
              onClick={() => openRenameDialog(selected)}
              disabled={busy}
            >
              <Pencil className="h-4 w-4" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              className="h-8 w-8 text-destructive hover:text-destructive"
              aria-label="删除"
              onClick={() => openDeleteDialog(selected)}
              disabled={busy}
            >
              <Trash2 className="h-4 w-4" />
            </Button>
          </div>
        ) : null}
      </div>

      {dialog ? (
        <ActionDialogPanel
          dialog={dialog}
          error={dialogError}
          busy={busy}
          exportFolders={exportFolders}
          onChange={(next) => {
            setDialog(next);
            setDialogError(null);
          }}
          onCancel={() => {
            setDialog(null);
            setDialogError(null);
          }}
          onConfirm={submitDialog}
        />
      ) : null}

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
            void onOpenFile(menuTarget);
          } else if (id === "export") {
            openExportDialog(menuTarget);
          } else if (id === "rename") {
            openRenameDialog(menuTarget);
          } else if (id === "delete") {
            openDeleteDialog(menuTarget);
          }
        }}
        onClose={() => setMenu({ open: false, x: 0, y: 0 })}
      />
    </div>
  );
}
