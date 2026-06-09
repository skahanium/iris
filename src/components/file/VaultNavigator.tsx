import { useVirtualizer } from "@tanstack/react-virtual";
import {
  BookMarked,
  ChevronRight,
  FileStack,
  FileText,
  Folder,
  FolderInput,
  FolderPlus,
  LibraryBig,
  Lock,
  LockOpen,
  MoveRight,
  Pencil,
  Scale,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { TemplateEditor } from "@/components/file/TemplateEditor";
import {
  corpusUpsert,
  fileDelete,
  fileList,
  fileRename,
  fileSetLock,
  folderCreate,
  folderDelete,
  folderList,
  folderRename,
  knowledgeReindex,
  templateCreate,
  templateList,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { displayTitleForFileListItem } from "@/lib/note-display";
import {
  buildVaultTree,
  folderParentPath,
  joinVaultChildPath,
  listFilesInFolder,
  notePathInFolder,
  type VaultTreeNode,
} from "@/lib/vault-tree";
import { cn } from "@/lib/utils";
import type { FileListItem } from "@/types/ipc";

interface VaultNavigatorProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

type CorpusKind = "regulation" | "exemplar" | "general";

type RenameTarget =
  | { kind: "file"; file: FileListItem }
  | { kind: "folder"; path: string };

type MoveTarget =
  | { kind: "file"; file: FileListItem }
  | { kind: "files"; files: FileListItem[] }
  | { kind: "folder"; path: string };

interface CorpusKindOption {
  kind: CorpusKind;
  title: string;
  description: string;
  effect: string;
  scenesLabel: string;
  icon: LucideIcon;
}

const CORPUS_KIND_OPTIONS: CorpusKindOption[] = [
  {
    kind: "regulation",
    title: "法规库",
    description: "制度、条例、办法、条款说明等规范性资料。",
    effect: "法规结构索引会从这里抽取条款，知识查询会优先使用。",
    scenesLabel: "知识查询",
    icon: Scale,
  },
  {
    kind: "exemplar",
    title: "范文库",
    description: "模板、样例、优秀稿件、可复用写作参考。",
    effect: "写作辅助和范文学习会优先参考这些文档。",
    scenesLabel: "范文学习 / 写作辅助",
    icon: FileStack,
  },
  {
    kind: "general",
    title: "通用资料",
    description: "普通知识材料，只登记范围，不绑定专门场景。",
    effect: "可被手动选择为上下文范围，不改变专门索引策略。",
    scenesLabel: "手动选择",
    icon: LibraryBig,
  },
];
const DEFAULT_CORPUS_KIND_OPTION = CORPUS_KIND_OPTIONS[0]!;

function slugFromPath(prefix: string): string {
  return prefix
    .replace(/\\/g, "/")
    .replace(/\/$/, "")
    .split("/")
    .filter(Boolean)
    .join("_")
    .replace(/[^a-zA-Z0-9_\u4e00-\u9fff-]/g, "_")
    .toLowerCase();
}

function defaultScenesForKind(kind: CorpusKind): string[] {
  switch (kind) {
    case "regulation":
      return ["knowledge_lookup"];
    case "exemplar":
      return ["exemplar_learning", "drafting_assist"];
    default:
      return [];
  }
}

function isInvalidFolderName(name: string): boolean {
  return /[\\/:*?"<>|]/.test(name) || name === "." || name === "..";
}

function normalizeFolderPrefix(path: string): string {
  const normalized = path.replace(/\\/g, "/").replace(/^\/+/, "");
  if (!normalized) return "";
  return normalized.endsWith("/") ? normalized : `${normalized}/`;
}

function displayFolderPath(path: string): string {
  return path ? normalizeFolderPrefix(path) : "全部笔记";
}

function folderNameFromPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/$/, "").split("/").pop() ?? "";
}

function fileNameFromPath(path: string): string {
  return path.replace(/\\/g, "/").split("/").pop() ?? path;
}

function fileParentPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(0, index + 1) : "";
}

function normalizeDocumentName(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "";
  return trimmed.toLowerCase().endsWith(".md") ? trimmed : `${trimmed}.md`;
}

function isInvalidLeafName(name: string): boolean {
  return isInvalidFolderName(name) || name.includes("/") || name.includes("\\");
}

function buildFolderPath(parentPath: string, name: string): string {
  return joinVaultChildPath(parentPath, name);
}

function buildFolderPrefix(parentPath: string, name: string): string {
  return normalizeFolderPrefix(buildFolderPath(parentPath, name));
}

function availableMoveFolders(
  folders: string[],
  target: MoveTarget | null,
): string[] {
  const normalized = Array.from(
    new Set(folders.map(normalizeFolderPrefix).filter(Boolean)),
  ).sort((a, b) => a.localeCompare(b, "zh-Hans-CN"));
  if (!target || target.kind === "file" || target.kind === "files") {
    return ["", ...normalized];
  }
  const current = normalizeFolderPrefix(target.path);
  return [
    "",
    ...normalized.filter(
      (folder) => folder !== current && !folder.startsWith(current),
    ),
  ];
}

function FolderCreateDialog({
  open,
  parentPath,
  onCancel,
  onSubmit,
}: {
  open: boolean;
  parentPath: string;
  onCancel: () => void;
  onSubmit: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const trimmed = name.trim();
  const invalid = Boolean(trimmed) && isInvalidLeafName(trimmed);
  const preview =
    trimmed && !invalid ? buildFolderPrefix(parentPath, trimmed) : "";

  useEffect(() => {
    if (open) setName("");
  }, [open]);

  const submit = () => {
    if (!trimmed || invalid) return;
    onSubmit(trimmed);
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent size="compact" className="max-w-xl">
        <DialogHeader>
          <DialogTitle>新建文件夹</DialogTitle>
          <DialogDescription>
            在选中的位置创建一个新的笔记文件夹。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 px-4 pb-2">
          <div className="grid gap-2 rounded-lg border border-border/70 bg-surface-inset/40 px-3 py-2 text-xs">
            <div>
              <div className="text-[11px] font-medium text-muted-foreground">
                父级位置
              </div>
              <div className="mt-1 break-all font-mono text-foreground">
                {displayFolderPath(parentPath)}
              </div>
            </div>
            <div>
              <div className="text-[11px] font-medium text-muted-foreground">
                最终路径
              </div>
              <div className="mt-1 break-all font-mono text-foreground">
                {preview || "输入名称后预览"}
              </div>
            </div>
          </div>
          <label className="block space-y-1">
            <span className="text-xs font-medium text-muted-foreground">
              文件夹名称
            </span>
            <Input
              aria-label="文件夹名称"
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  submit();
                }
              }}
            />
          </label>
          {invalid ? (
            <p className="text-xs text-destructive">
              名称不能包含路径分隔符或 Windows 非法字符。
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onCancel}>
            取消
          </Button>
          <Button type="button" onClick={submit} disabled={!trimmed || invalid}>
            创建文件夹
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RenameItemDialog({
  target,
  onCancel,
  onSubmit,
}: {
  target: RenameTarget | null;
  onCancel: () => void;
  onSubmit: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const isFile = target?.kind === "file";
  const parent =
    target?.kind === "file"
      ? fileParentPath(target.file.path)
      : target?.kind === "folder"
        ? folderParentPath(target.path)
        : "";
  const trimmed = name.trim();
  const normalizedName = isFile ? normalizeDocumentName(trimmed) : trimmed;
  const invalid = Boolean(trimmed) && isInvalidLeafName(normalizedName);
  const preview =
    target && trimmed && !invalid
      ? target.kind === "file"
        ? joinVaultChildPath(parent, normalizedName)
        : buildFolderPrefix(parent, normalizedName)
      : "";

  useEffect(() => {
    if (!target) return;
    setName(
      target.kind === "file"
        ? fileNameFromPath(target.file.path)
        : folderNameFromPath(target.path),
    );
  }, [target]);

  const submit = () => {
    if (!trimmed || invalid) return;
    onSubmit(normalizedName);
  };

  return (
    <Dialog open={target !== null} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent size="compact" className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{isFile ? "重命名文档" : "重命名文件夹"}</DialogTitle>
          <DialogDescription>
            只填写名称，路径会根据当前位置自动生成。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 px-4 pb-2">
          <div className="rounded-lg border border-border/70 bg-surface-inset/40 px-3 py-2 text-xs">
            <div className="text-[11px] font-medium text-muted-foreground">
              所在位置
            </div>
            <div className="mt-1 break-all font-mono text-foreground">
              {displayFolderPath(parent)}
            </div>
            <div className="mt-2 text-[11px] font-medium text-muted-foreground">
              新路径
            </div>
            <div className="mt-1 break-all font-mono text-foreground">
              {preview || "输入名称后预览"}
            </div>
          </div>
          <label className="block space-y-1">
            <span className="text-xs font-medium text-muted-foreground">
              {isFile ? "文档名称" : "文件夹名称"}
            </span>
            <Input
              aria-label={isFile ? "文档名称" : "文件夹名称"}
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  submit();
                }
              }}
            />
          </label>
          {invalid ? (
            <p className="text-xs text-destructive">
              名称不能包含路径分隔符或 Windows 非法字符。
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onCancel}>
            取消
          </Button>
          <Button type="button" onClick={submit} disabled={!trimmed || invalid}>
            保存名称
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function MoveItemDialog({
  target,
  folders,
  onCancel,
  onSubmit,
}: {
  target: MoveTarget | null;
  folders: string[];
  onCancel: () => void;
  onSubmit: (folderPath: string) => void;
}) {
  const options = useMemo(
    () => availableMoveFolders(folders, target),
    [folders, target],
  );
  const initialTarget =
    target?.kind === "file"
      ? fileParentPath(target.file.path)
      : target?.kind === "files"
        ? fileParentPath(target.files[0]?.path ?? "")
        : target?.kind === "folder"
          ? folderParentPath(target.path)
          : "";
  const [selected, setSelected] = useState("");

  useEffect(() => {
    if (target) setSelected(initialTarget);
  }, [initialTarget, target]);

  const targetName =
    target?.kind === "file"
      ? fileNameFromPath(target.file.path)
      : target?.kind === "files"
        ? `${target.files.length} 个文档`
        : target?.kind === "folder"
          ? folderNameFromPath(target.path)
          : "";
  const preview =
    target && targetName
      ? target.kind === "file"
        ? joinVaultChildPath(selected, targetName)
        : target.kind === "files"
          ? `${displayFolderPath(selected)} / ${targetName}`
          : buildFolderPrefix(selected, targetName)
      : "";

  return (
    <Dialog open={target !== null} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent size="compact" className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {target?.kind === "folder"
              ? "移动文件夹"
              : target?.kind === "files"
                ? "批量移动文档"
                : "移动文档"}
          </DialogTitle>
          <DialogDescription>
            选择目标文件夹，Iris 会自动保留原名称。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 px-4 pb-2">
          <div className="rounded-lg border border-border/70 bg-surface-inset/40 px-3 py-2 text-xs">
            <div className="text-[11px] font-medium text-muted-foreground">
              选择目标文件夹
            </div>
            <div className="mt-2 grid max-h-44 gap-1 overflow-auto">
              {options.map((folder) => {
                const active = folder === selected;
                return (
                  <button
                    key={folder || "__root__"}
                    type="button"
                    className={cn(
                      "rounded-md px-2 py-1.5 text-left font-mono text-xs transition-colors",
                      active
                        ? "bg-primary/10 text-primary"
                        : "hover:bg-surface-inset",
                    )}
                    onClick={() => setSelected(folder)}
                  >
                    {folder || "全部笔记"}
                  </button>
                );
              })}
            </div>
          </div>
          <div className="rounded-lg border border-border/70 px-3 py-2 text-xs">
            <div className="text-[11px] font-medium text-muted-foreground">
              移动后路径
            </div>
            <div className="mt-1 break-all font-mono text-foreground">
              {preview}
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onCancel}>
            取消
          </Button>
          <Button type="button" onClick={() => onSubmit(selected)}>
            移动到此处
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function TreeFolder({
  node,
  depth,
  selected,
  expanded,
  onSelect,
  onToggle,
}: {
  node: VaultTreeNode;
  depth: number;
  selected: string;
  expanded: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}) {
  if (node.kind !== "folder") return null;
  const isOpen = expanded.has(node.path);
  const isSelected = selected === node.path;

  return (
    <div>
      <div
        className={cn(
          "group flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-xs hover:bg-accent",
          isSelected && "bg-accent font-medium text-accent-foreground",
        )}
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
      >
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-1 text-left"
          onClick={() => {
            onSelect(node.path);
            onToggle(node.path);
          }}
        >
          <ChevronRight
            className={cn(
              "h-3 w-3 shrink-0 transition-transform",
              isOpen && "rotate-90",
            )}
          />
          <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
        </button>
      </div>
      {isOpen &&
        node.children?.map((child) =>
          child.kind === "folder" ? (
            <TreeFolder
              key={child.path}
              node={child}
              depth={depth + 1}
              selected={selected}
              expanded={expanded}
              onSelect={onSelect}
              onToggle={onToggle}
            />
          ) : null,
        )}
    </div>
  );
}

export function VaultNavigator({ open, onClose, onOpen }: VaultNavigatorProps) {
  const [files, setFiles] = useState<FileListItem[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState("未命名文档.md");
  const [templates, setTemplates] = useState<{ name: string }[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null);
  const [moveTarget, setMoveTarget] = useState<MoveTarget | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);
  const [batchDeleteTarget, setBatchDeleteTarget] = useState<FileListItem[]>(
    [],
  );
  const [batchMode, setBatchMode] = useState(false);
  const [selectedFilePaths, setSelectedFilePaths] = useState<Set<string>>(
    new Set(),
  );
  const [selectedFolder, setSelectedFolder] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [corpusKind, setCorpusKind] = useState<CorpusKind>("regulation");
  const [corpusSaving, setCorpusSaving] = useState(false);
  const [folderCreateOpen, setFolderCreateOpen] = useState(false);
  const [folderCreateParent, setFolderCreateParent] = useState("");
  const [folderDeleteTarget, setFolderDeleteTarget] = useState<{
    path: string;
    name: string;
  } | null>(null);
  const [editingTemplate, setEditingTemplate] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => buildVaultTree(files, folders), [files, folders]);
  const folderFiles = useMemo(
    () => listFilesInFolder(files, selectedFolder),
    [files, selectedFolder],
  );
  const selectedFiles = useMemo(
    () => folderFiles.filter((file) => selectedFilePaths.has(file.path)),
    [folderFiles, selectedFilePaths],
  );
  const allFolderFilesSelected =
    folderFiles.length > 0 && selectedFiles.length === folderFiles.length;

  const virtualizer = useVirtualizer({
    count: folderFiles.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 40,
    overscan: 10,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const renderedFileItems =
    virtualItems.length > 0
      ? virtualItems
      : folderFiles.map((file, index) => ({
          index,
          key: file.path,
          size: 40,
          start: index * 40,
        }));
  const fileListHeight =
    virtualItems.length > 0
      ? virtualizer.getTotalSize()
      : folderFiles.length * 40;

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    void Promise.all([fileList(), folderList()])
      .then(([nextFiles, nextFolders]) => {
        setFiles(nextFiles);
        setFolders(nextFolders);
      })
      .catch((e) =>
        setError(e instanceof Error ? e.message : "加载文件列表失败"),
      )
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (open) {
      refresh();
      void templateList().then(setTemplates);
      setShowTemplates(false);
    }
  }, [open, refresh]);

  useEffect(() => {
    setSelectedFilePaths(new Set());
    setBatchMode(false);
  }, [selectedFolder]);

  useEffect(() => {
    setSelectedFilePaths((prev) => {
      const visible = new Set(folderFiles.map((file) => file.path));
      const next = new Set(
        Array.from(prev).filter((path) => visible.has(path)),
      );
      return next.size === prev.size ? prev : next;
    });
  }, [folderFiles]);

  const toggleExpanded = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const toggleFileSelected = useCallback((path: string) => {
    setSelectedFilePaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const handleFolderCreate = useCallback(
    async (name: string) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      if (isInvalidFolderName(trimmed)) {
        setError("文件夹名称不能包含路径分隔符或非法字符");
        return;
      }
      const folderPath = joinVaultChildPath(folderCreateParent, trimmed);
      try {
        await folderCreate(folderPath);
        setFolderCreateOpen(false);
        setFolderCreateParent("");
        setSelectedFolder(`${folderPath.replace(/\\/g, "/")}/`);
        setExpanded((prev) =>
          new Set(prev).add(`${folderPath.replace(/\\/g, "/")}/`),
        );
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "创建文件夹失败");
      }
    },
    [folderCreateParent, refresh],
  );

  const handleRename = useCallback(
    async (name: string) => {
      if (!renameTarget) return;
      try {
        if (renameTarget.kind === "file") {
          const parent = fileParentPath(renameTarget.file.path);
          const nextPath = joinVaultChildPath(
            parent,
            normalizeDocumentName(name),
          );
          if (nextPath !== renameTarget.file.path) {
            await fileRename(renameTarget.file.path, nextPath);
          }
        } else {
          const parent = folderParentPath(renameTarget.path);
          const nextPath = buildFolderPath(parent, name);
          if (nextPath !== renameTarget.path.replace(/\/$/, "")) {
            await folderRename(renameTarget.path, nextPath);
            setSelectedFolder(normalizeFolderPrefix(nextPath));
          }
        }
        setRenameTarget(null);
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "重命名失败");
      }
    },
    [refresh, renameTarget],
  );

  const handleMove = useCallback(
    async (targetFolder: string) => {
      if (!moveTarget) return;
      try {
        if (moveTarget.kind === "file") {
          const nextPath = joinVaultChildPath(
            targetFolder,
            fileNameFromPath(moveTarget.file.path),
          );
          if (nextPath !== moveTarget.file.path) {
            await fileRename(moveTarget.file.path, nextPath);
          }
        } else if (moveTarget.kind === "files") {
          await Promise.all(
            moveTarget.files.map((file) => {
              const nextPath = joinVaultChildPath(
                targetFolder,
                fileNameFromPath(file.path),
              );
              return nextPath === file.path
                ? Promise.resolve()
                : fileRename(file.path, nextPath);
            }),
          );
        } else {
          const nextPath = buildFolderPath(
            targetFolder,
            folderNameFromPath(moveTarget.path),
          );
          if (nextPath !== moveTarget.path.replace(/\/$/, "")) {
            await folderRename(moveTarget.path, nextPath);
            setSelectedFolder(normalizeFolderPrefix(nextPath));
          }
        }
        setMoveTarget(null);
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "移动失败");
      }
    },
    [moveTarget, refresh],
  );

  const handleBatchSetLock = useCallback(
    async (locked: boolean) => {
      if (selectedFiles.length === 0) return;
      try {
        await Promise.all(
          selectedFiles.map((file) => fileSetLock(file.path, locked)),
        );
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "批量更新锁定状态失败");
      }
    },
    [refresh, selectedFiles],
  );

  const handleBatchDelete = useCallback(async () => {
    if (batchDeleteTarget.length === 0) return;
    try {
      await Promise.all(batchDeleteTarget.map((file) => fileDelete(file.path)));
      setBatchDeleteTarget([]);
      setSelectedFilePaths(new Set());
      setBatchMode(false);
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "批量删除失败");
    }
  }, [batchDeleteTarget, refresh]);

  const handleFolderDelete = useCallback(async () => {
    if (!folderDeleteTarget) return;
    try {
      await folderDelete(folderDeleteTarget.path);
      setFolderDeleteTarget(null);
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "删除文件夹失败");
    }
  }, [folderDeleteTarget, refresh]);

  const createFromTemplate = async (tmplName: string) => {
    const path = notePathInFolder(selectedFolder, newName);
    await templateCreate(path, tmplName);
    refresh();
    onOpen(path);
    setShowTemplates(false);
  };

  const handleSetCorpus = async (kind: CorpusKind) => {
    const prefix = selectedFolder.endsWith("/")
      ? selectedFolder
      : `${selectedFolder}/`;
    if (!prefix || prefix === "/") return;
    const name =
      prefix.replace(/\/$/, "").split("/").pop() ?? prefix.replace(/\/$/, "");
    setCorpusSaving(true);
    try {
      await corpusUpsert({
        id: slugFromPath(prefix) || "corpus",
        name,
        pathPrefix: prefix,
        kind,
        scenes: defaultScenesForKind(kind),
      });
      await knowledgeReindex();
    } catch (e) {
      setError(e instanceof Error ? e.message : "设置语料库失败");
    } finally {
      setCorpusSaving(false);
    }
  };

  const selectedFolderName = selectedFolder
    ? folderNameFromPath(selectedFolder)
    : "";
  const selectedCorpusOption =
    CORPUS_KIND_OPTIONS.find((option) => option.kind === corpusKind) ??
    DEFAULT_CORPUS_KIND_OPTION;
  const deleteDialogOpen =
    deleteTarget !== null ||
    batchDeleteTarget.length > 0 ||
    folderDeleteTarget !== null;

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title="浏览笔记库"
        size="command"
      >
        <div className="flex gap-2 border-b border-border/60 bg-surface-inset/30 px-4 py-2">
          <Input
            className="text-xs"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
          />
          <Button
            type="button"
            size="icon"
            variant="outline"
            title="新建笔记"
            onClick={async () => {
              if (showTemplates && templates.length > 0) return;
              const trimmed = newName.trim();
              const created = await createDefaultNote({
                folderPrefix: selectedFolder,
                titleHint: trimmed,
              });
              refresh();
              onOpen(created.path);
            }}
          >
            <FolderPlus className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            size="icon"
            variant="outline"
            title="新建文件夹"
            onClick={() => {
              setFolderCreateParent(selectedFolder);
              setFolderCreateOpen(true);
            }}
          >
            <Folder className="h-4 w-4" />
          </Button>
        </div>
        {templates.length > 0 && (
          <div className="border-b border-border px-4 pb-2">
            <button
              type="button"
              className="text-xs text-muted-foreground hover:text-primary"
              onClick={() => setShowTemplates(!showTemplates)}
            >
              {showTemplates ? "收起模板" : "从模板新建…"}
            </button>
            {showTemplates && (
              <div className="mt-1 flex flex-wrap gap-1">
                {templates.map((t) => (
                  <div key={t.name} className="flex items-center gap-0.5">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="text-xs"
                      onClick={() => void createFromTemplate(t.name)}
                    >
                      {t.name}
                    </Button>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      className="h-6 w-6"
                      title="编辑模板"
                      onClick={() => setEditingTemplate(t.name)}
                    >
                      <Pencil className="h-3 w-3" />
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
        {error && <p className="px-3 py-2 text-xs text-destructive">{error}</p>}
        <div className="flex min-h-0 flex-1">
          <div className="w-44 shrink-0 overflow-y-auto border-r border-border/60 p-2">
            <button
              type="button"
              className={cn(
                "mb-1 flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-xs hover:bg-accent",
                !selectedFolder && "bg-accent font-medium",
              )}
              onClick={() => setSelectedFolder("")}
            >
              <Folder className="h-3.5 w-3.5" />
              全部笔记
            </button>
            {tree.map((node) =>
              node.kind === "folder" ? (
                <TreeFolder
                  key={node.path}
                  node={node}
                  depth={0}
                  selected={selectedFolder}
                  expanded={expanded}
                  onSelect={setSelectedFolder}
                  onToggle={toggleExpanded}
                />
              ) : null,
            )}
          </div>
          <div ref={parentRef} className="min-h-0 flex-1 overflow-auto">
            {selectedFolder ? (
              <div
                data-testid="folder-details"
                data-density="compact"
                className="border-b border-border/60 bg-surface-inset/20 px-3 py-2"
              >
                <div className="grid gap-2 xl:grid-cols-[minmax(12rem,1fr)_auto] xl:items-start">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                      <div className="text-xs font-semibold text-foreground">
                        文件夹详情
                      </div>
                      <div className="min-w-0 break-all font-mono text-[11px] text-muted-foreground">
                        {selectedFolder}
                      </div>
                    </div>
                  </div>
                  {!deleteDialogOpen ? (
                    <div className="flex flex-wrap gap-1.5">
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8 px-2 text-xs"
                        title="重命名文件夹"
                        onClick={() =>
                          setRenameTarget({
                            kind: "folder",
                            path: selectedFolder,
                          })
                        }
                      >
                        <Pencil className="h-3.5 w-3.5" />
                        重命名文件夹
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8 px-2 text-xs"
                        title="移动文件夹"
                        onClick={() =>
                          setMoveTarget({
                            kind: "folder",
                            path: selectedFolder,
                          })
                        }
                      >
                        <FolderInput className="h-3.5 w-3.5" />
                        移动文件夹
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8 px-2 text-xs hover:border-destructive/50 hover:text-destructive"
                        title="删除文件夹"
                        onClick={() =>
                          setFolderDeleteTarget({
                            path: selectedFolder,
                            name: selectedFolderName,
                          })
                        }
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        删除文件夹
                      </Button>
                    </div>
                  ) : null}
                </div>
                <div
                  data-testid="corpus-kind-select"
                  data-layout="dropdown"
                  className="mt-2 rounded-md border border-border/60 bg-panel/70 px-2.5 py-1.5"
                >
                  <div className="grid gap-2 sm:grid-cols-[auto_minmax(10rem,16rem)_auto] sm:items-center">
                    <span className="text-xs font-semibold text-foreground">
                      语料库类型
                    </span>
                    <Select
                      value={corpusKind}
                      onValueChange={(value) =>
                        setCorpusKind(value as CorpusKind)
                      }
                    >
                      <SelectTrigger
                        className="h-8 text-xs"
                        aria-label="语料库类型"
                        title={selectedCorpusOption.effect}
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {CORPUS_KIND_OPTIONS.map((option) => (
                          <SelectItem
                            key={option.kind}
                            value={option.kind}
                            title={option.effect}
                          >
                            {option.title}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Button
                      type="button"
                      size="sm"
                      className="h-8 px-2.5 text-xs"
                      onClick={() => void handleSetCorpus(corpusKind)}
                      disabled={corpusSaving}
                    >
                      <BookMarked className="h-3.5 w-3.5" />
                      {corpusSaving ? "设置中" : "确认设置"}
                    </Button>
                  </div>
                </div>
              </div>
            ) : null}
            {folderFiles.length > 0 && !deleteDialogOpen ? (
              <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-panel px-3 py-2">
                <div className="text-xs text-muted-foreground">
                  {batchMode ? `已选 ${selectedFiles.length} 个文档` : null}
                </div>
                <div className="flex flex-wrap items-center gap-1.5">
                  {batchMode ? (
                    <>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() =>
                          setSelectedFilePaths(
                            allFolderFilesSelected
                              ? new Set()
                              : new Set(folderFiles.map((file) => file.path)),
                          )
                        }
                      >
                        {allFolderFilesSelected ? "清空" : "全选"}
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={selectedFiles.length === 0}
                        onClick={() =>
                          setMoveTarget({
                            kind: "files",
                            files: selectedFiles,
                          })
                        }
                      >
                        <MoveRight className="h-3.5 w-3.5" />
                        批量移动
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={selectedFiles.length === 0}
                        onClick={() => void handleBatchSetLock(true)}
                      >
                        <Lock className="h-3.5 w-3.5" />
                        批量锁定
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={selectedFiles.length === 0}
                        onClick={() => void handleBatchSetLock(false)}
                      >
                        <LockOpen className="h-3.5 w-3.5" />
                        批量解锁
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="hover:border-destructive/50 hover:text-destructive"
                        disabled={selectedFiles.length === 0}
                        onClick={() => setBatchDeleteTarget(selectedFiles)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        批量删除
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                          setSelectedFilePaths(new Set());
                          setBatchMode(false);
                        }}
                      >
                        退出
                      </Button>
                    </>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => setBatchMode(true)}
                    >
                      批量操作
                    </Button>
                  )}
                </div>
              </div>
            ) : null}
            {loading ? (
              <p className="p-3 text-xs text-muted-foreground">加载中…</p>
            ) : folderFiles.length === 0 ? (
              <p className="p-3 text-xs text-muted-foreground">
                {selectedFolder ? "此文件夹暂无笔记" : "暂无笔记"}
              </p>
            ) : (
              <div
                style={{
                  height: `${fileListHeight}px`,
                  position: "relative",
                }}
              >
                {renderedFileItems.map((virtualItem) => {
                  const f = folderFiles[virtualItem.index]!;
                  return (
                    <div
                      key={f.path}
                      style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: "100%",
                        height: `${virtualItem.size}px`,
                        transform: `translateY(${virtualItem.start}px)`,
                      }}
                      className="flex items-center gap-1 border-b border-border/50 px-2 py-1.5 text-sm"
                    >
                      {batchMode ? (
                        <input
                          type="checkbox"
                          aria-label={`选择文档 ${displayTitleForFileListItem(f)}`}
                          checked={selectedFilePaths.has(f.path)}
                          className="h-3.5 w-3.5 shrink-0 rounded border-border"
                          onChange={() => toggleFileSelected(f.path)}
                          onClick={(event) => event.stopPropagation()}
                        />
                      ) : null}
                      <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      <button
                        type="button"
                        className="min-w-0 flex-1 truncate text-left hover:text-primary"
                        onClick={() => {
                          onOpen(f.path);
                          onClose();
                        }}
                      >
                        {displayTitleForFileListItem(f)}
                      </button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        title="重命名文档"
                        aria-label="重命名文档"
                        onClick={() =>
                          setRenameTarget({ kind: "file", file: f })
                        }
                      >
                        <Pencil className="h-3 w-3" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        title="移动文档"
                        aria-label="移动文档"
                        onClick={() => setMoveTarget({ kind: "file", file: f })}
                      >
                        <MoveRight className="h-3 w-3" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        title={f.isLocked ? "解锁编辑" : "锁定编辑"}
                        onClick={async () => {
                          const next = !f.isLocked;
                          await fileSetLock(f.path, next);
                          refresh();
                        }}
                      >
                        {f.isLocked ? (
                          <Lock className="h-3 w-3" />
                        ) : (
                          <LockOpen className="h-3 w-3" />
                        )}
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        title="移入回收站"
                        aria-label="移入回收站"
                        onClick={() => setDeleteTarget(f)}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </IrisOverlay>

      <FolderCreateDialog
        open={folderCreateOpen}
        parentPath={folderCreateParent}
        onCancel={() => {
          setFolderCreateOpen(false);
          setFolderCreateParent("");
        }}
        onSubmit={handleFolderCreate}
      />

      <RenameItemDialog
        target={renameTarget}
        onCancel={() => setRenameTarget(null)}
        onSubmit={(name) => void handleRename(name)}
      />

      <MoveItemDialog
        target={moveTarget}
        folders={folders}
        onCancel={() => setMoveTarget(null)}
        onSubmit={(folderPath) => void handleMove(folderPath)}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除文件"
        message={`确定删除「${deleteTarget?.title ?? deleteTarget?.path ?? ""}」？`}
        description="正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。"
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={async () => {
          if (!deleteTarget) return;
          await fileDelete(deleteTarget.path);
          setDeleteTarget(null);
          refresh();
        }}
      />

      <ConfirmDialog
        open={batchDeleteTarget.length > 0}
        title="批量删除文档"
        message={`确定删除 ${batchDeleteTarget.length} 个文档？`}
        description="正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。"
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setBatchDeleteTarget([])}
        onConfirm={handleBatchDelete}
      />

      <ConfirmDialog
        open={folderDeleteTarget !== null}
        title="删除文件夹"
        message={`确定删除文件夹「${folderDeleteTarget?.name ?? ""}」？`}
        description="只能删除空文件夹。"
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setFolderDeleteTarget(null)}
        onConfirm={handleFolderDelete}
      />

      <TemplateEditor
        open={editingTemplate !== null}
        templateName={editingTemplate}
        onClose={() => setEditingTemplate(null)}
        onSaved={() => {
          void templateList().then(setTemplates);
        }}
      />
    </>
  );
}
