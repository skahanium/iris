import { useVirtualizer } from "@tanstack/react-virtual";
import {
  BookMarked,
  ChevronRight,
  FileImage,
  FileStack,
  FileText,
  FileVideo,
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
  corpusList,
  corpusUpsert,
  fileDelete,
  fileRename,
  fileSetLock,
  folderCreate,
  folderDelete,
  folderList,
  folderRename,
  knowledgeReindex,
  templateCreate,
  templateList,
  workspaceList,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import {
  prepareNoteOpenFromContent,
  type NoteOpenBudgetKind,
  type PrepareNoteOpenRequest,
  type PreparedNoteOpen,
} from "@/lib/note-open-preparation";
import {
  allocateAvailableNotePath,
  DEFAULT_NEW_DOCUMENT_TITLE,
  isAutoSyncableNotePath,
  isPlaceholderDocumentTitle,
  titleToNotePath,
} from "@/lib/note-names";
import { displayTitleForFileListItem } from "@/lib/note-display";
import {
  buildVaultTree,
  folderParentPath,
  joinVaultChildPath,
  listFilesInFolder,
  type VaultTreeNode,
} from "@/lib/vault-tree";
import { cn } from "@/lib/utils";
import type { NoteOpenSource } from "@/lib/document-open-runtime";
import type { CorpusListItem, FileListItem, WorkspaceItem } from "@/types/ipc";

import {
  FolderCreateDialog,
  MoveItemDialog,
  RenameItemDialog,
} from "./VaultNavigatorDialogs";
import {
  buildFolderPath,
  canonicalCorpusKind,
  defaultScenesForKind,
  displayFolderPath,
  fileNameFromPath,
  fileParentPath,
  folderNameFromPath,
  isInvalidFolderName,
  normalizeDocumentName,
  normalizeFolderPrefix,
  slugFromPath,
  type CorpusKind,
  type MoveTarget,
  type RenameTarget,
} from "./vault-navigator-model";

type VaultFileItem = FileListItem & {
  kind?: WorkspaceItem["kind"];
  mediaKind?: WorkspaceItem["mediaKind"];
  mimeType?: string | null;
};

interface VaultNavigatorOpenOptions {
  openBudgetKind?: NoteOpenBudgetKind;
  openStartedAt?: number;
  openTraceRequest?: PrepareNoteOpenRequest;
  preparedNote?: PreparedNoteOpen;
  priority?: "foreground" | "hot" | "warm" | "background";
  titleHint?: string;
}

function vaultFileItem(item: WorkspaceItem): VaultFileItem {
  return {
    isLocked: item.isLocked,
    kind: item.kind,
    mediaKind: item.mediaKind,
    mimeType: item.mimeType,
    path: item.path,
    title: item.title,
    updatedAt: item.updatedAt ?? "",
  };
}

function isNoteFile(file: VaultFileItem): boolean {
  return !file.kind || file.kind === "note";
}

function noteListItem(file: VaultFileItem): FileListItem | null {
  if (!isNoteFile(file)) return null;
  return {
    isLocked: file.isLocked,
    path: file.path,
    title: file.title,
    updatedAt: file.updatedAt,
  };
}

function vaultFileTitle(file: VaultFileItem): string {
  if (isNoteFile(file)) return displayTitleForFileListItem(file);
  return file.title || file.path.split("/").pop() || file.path;
}

function vaultFileIcon(file: VaultFileItem): LucideIcon {
  if (file.mediaKind === "image") return FileImage;
  if (file.mediaKind === "video") return FileVideo;
  return FileText;
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}

interface VaultNavigatorProps {
  open: boolean;
  onClose: () => void;
  onOpen: (
    path: string,
    source: NoteOpenSource,
    options?: VaultNavigatorOpenOptions,
  ) => void | Promise<void>;
  onPrepare?: (file: FileListItem, source: NoteOpenSource) => void;
  onBeforeFilePathChange?: (path: string) => Promise<void>;
  onFilePathChanged?: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  onBeforeFileDelete?: (path: string) => Promise<void>;
  onFileDeleted?: (path: string) => void;
  onIndexChange?: () => void;
}

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
    kind: "authority",
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
    kind: "reference",
    title: "通用资料",
    description: "普通知识材料，只登记范围，不绑定专门场景。",
    effect: "可被手动选择为上下文范围，不改变专门索引策略。",
    scenesLabel: "手动选择",
    icon: LibraryBig,
  },
  {
    kind: "lookup",
    title: "查阅资料",
    description: "低权威或临时资料，可了解其内容，但不应作为依据。",
    effect:
      "AI 可摘要其内容，但必须标注仅供查阅，不能据此形成结论或规范性判断。",
    scenesLabel: "查阅 / 研究",
    icon: FileText,
  },
];
CORPUS_KIND_OPTIONS.splice(
  0,
  CORPUS_KIND_OPTIONS.length,
  {
    kind: "authority",
    title: "规范依据",
    description: "法规、制度、政策、纪律条文等必须优先遵循的材料。",
    effect: "AI 必须优先遵循，可作为结论依据，并在查阅、研究和写作时优先检索。",
    scenesLabel: "查阅 / 研究 / 写作",
    icon: Scale,
  },
  {
    kind: "exemplar",
    title: "范文样本",
    description: "优秀公文、报告、请示、通知等用于学习写法的材料。",
    effect: "AI 只学习结构、语气和表达方式，不把其中事实或结论当依据。",
    scenesLabel: "范文学习 / 写作",
    icon: FileStack,
  },
  {
    kind: "reference",
    title: "参考资料",
    description: "背景材料、调研材料、说明资料等可作为背景参考的材料。",
    effect: "AI 可查询、摘要和引用为背景参考，但不能当作规范遵循。",
    scenesLabel: "查阅 / 研究",
    icon: LibraryBig,
  },
  {
    kind: "lookup",
    title: "查阅资料",
    description: "低权威或临时资料，可了解其内容，但不应作为依据。",
    effect:
      "AI 可摘要其内容，但必须标注仅供查阅，不能据此形成结论或规范性判断。",
    scenesLabel: "查阅 / 研究",
    icon: FileText,
  },
);
const DEFAULT_CORPUS_KIND_OPTION = CORPUS_KIND_OPTIONS[0]!;
const PREPARE_FOLDER_LIMIT = 8;

function VaultNavigatorLoadingSkeleton() {
  return (
    <div
      className="space-y-2 p-3"
      aria-live="polite"
      role="status"
      aria-label="笔记库加载中"
    >
      {Array.from({ length: 8 }).map((_, index) => (
        <div
          key={index}
          className="flex h-9 items-center gap-2 rounded-md border border-border/40 bg-surface-inset/35 px-2"
        >
          <span className="h-3.5 w-3.5 rounded bg-muted/60" />
          <span className="h-2.5 flex-1 rounded bg-muted/50" />
          <span className="h-2.5 w-10 rounded bg-muted/35" />
        </div>
      ))}
    </div>
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

export function VaultNavigatorBody({
  open,
  onClose,
  onOpen,
  onPrepare,
  onBeforeFilePathChange,
  onFilePathChanged,
  onBeforeFileDelete,
  onFileDeleted,
  onIndexChange,
}: VaultNavigatorProps) {
  const [files, setFiles] = useState<VaultFileItem[]>([]);
  const [folders, setFolders] = useState<string[]>([]);
  const [corpora, setCorpora] = useState<CorpusListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
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
  const [corpusKind, setCorpusKind] = useState<CorpusKind>("authority");
  const [corpusSaving, setCorpusSaving] = useState(false);
  const [folderCreateOpen, setFolderCreateOpen] = useState(false);
  const [folderCreateParent, setFolderCreateParent] = useState("");
  const [folderDeleteTarget, setFolderDeleteTarget] = useState<{
    path: string;
    name: string;
  } | null>(null);
  const [editingTemplate, setEditingTemplate] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);
  const preparedKeysRef = useRef(new Set<string>());

  const tree = useMemo(() => buildVaultTree(files, folders), [files, folders]);
  const folderFiles = useMemo(
    () => listFilesInFolder(files, selectedFolder),
    [files, selectedFolder],
  );
  const selectedFiles = useMemo(
    () =>
      folderFiles.filter(
        (file) => isNoteFile(file) && selectedFilePaths.has(file.path),
      ),
    [folderFiles, selectedFilePaths],
  );
  const selectableFolderFiles = useMemo(
    () => folderFiles.filter(isNoteFile),
    [folderFiles],
  );
  const allFolderFilesSelected =
    selectableFolderFiles.length > 0 &&
    selectedFiles.length === selectableFolderFiles.length;

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
    void Promise.all([workspaceList(), folderList(), corpusList()])
      .then(([nextFiles, nextFolders, nextCorpora]) => {
        setFiles(nextFiles.map(vaultFileItem));
        setFolders(nextFolders);
        setCorpora(nextCorpora);
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
    } else {
      preparedKeysRef.current.clear();
    }
  }, [open, refresh]);

  useEffect(() => {
    setSelectedFilePaths(new Set());
    setBatchMode(false);
  }, [selectedFolder]);

  useEffect(() => {
    if (!selectedFolder) {
      setCorpusKind("authority");
      return;
    }
    const prefix = normalizeFolderPrefix(selectedFolder);
    const saved = corpora.find(
      (entry) => normalizeFolderPrefix(entry.pathPrefix) === prefix,
    );
    setCorpusKind(canonicalCorpusKind(saved?.kind ?? "authority"));
  }, [corpora, selectedFolder]);

  useEffect(() => {
    setSelectedFilePaths((prev) => {
      const visible = new Set(folderFiles.map((file) => file.path));
      const next = new Set(
        Array.from(prev).filter((path) => visible.has(path)),
      );
      return next.size === prev.size ? prev : next;
    });
  }, [folderFiles]);

  useEffect(() => {
    folderFiles.slice(0, PREPARE_FOLDER_LIMIT).forEach((file) => {
      const note = noteListItem(file);
      if (!note) return;
      const key = note.path;
      if (preparedKeysRef.current.has(key)) return;
      preparedKeysRef.current.add(key);
      onPrepare?.(note, "file-tree");
    });
  }, [folderFiles, onPrepare]);

  const toggleExpanded = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const toggleFileSelected = useCallback(
    (path: string) => {
      const target = folderFiles.find((file) => file.path === path);
      if (target && !isNoteFile(target)) return;
      setSelectedFilePaths((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
    },
    [folderFiles],
  );

  const preferredMoveFileName = useCallback((file: FileListItem) => {
    const title = displayTitleForFileListItem(file).trim();
    if (
      isAutoSyncableNotePath(file.path) &&
      title &&
      !isPlaceholderDocumentTitle(title)
    ) {
      return titleToNotePath(title);
    }
    return fileNameFromPath(file.path);
  }, []);

  const resolveMoveFilePath = useCallback(
    (
      file: FileListItem,
      targetFolder: string,
      reservedPaths?: Iterable<string>,
    ) =>
      allocateAvailableNotePath({
        files: files.filter(isNoteFile),
        folderPrefix: targetFolder,
        preferredFileName: preferredMoveFileName(file),
        excludePaths: [file.path],
        reservedPaths,
      }),
    [files, preferredMoveFileName],
  );

  const movePreviewPath = useCallback(
    (target: MoveTarget | null, targetFolder: string): string => {
      if (!target) return "";
      if (target.kind === "file") {
        return resolveMoveFilePath(target.file, targetFolder);
      }
      if (target.kind === "files") {
        return `${displayFolderPath(targetFolder)} / ${target.files.length} 个文档`;
      }
      return buildFolderPath(targetFolder, folderNameFromPath(target.path));
    },
    [resolveMoveFilePath],
  );

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
        onIndexChange?.();
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "创建文件夹失败");
      }
    },
    [folderCreateParent, onIndexChange, refresh],
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
            await onBeforeFilePathChange?.(renameTarget.file.path);
            await fileRename(renameTarget.file.path, nextPath);
            onFilePathChanged?.(renameTarget.file.path, nextPath, name);
          }
        } else {
          const parent = folderParentPath(renameTarget.path);
          const nextPath = buildFolderPath(parent, name);
          if (nextPath !== renameTarget.path.replace(/\/$/, "")) {
            const oldPrefix = normalizeFolderPrefix(renameTarget.path);
            const newPrefix = normalizeFolderPrefix(nextPath);
            const renamedFiles = files.filter((file) =>
              file.path.startsWith(oldPrefix),
            );
            for (const file of renamedFiles) {
              await onBeforeFilePathChange?.(file.path);
            }
            await folderRename(renameTarget.path, nextPath);
            for (const file of renamedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              onFilePathChanged?.(
                file.path,
                remappedPath,
                vaultFileTitle(file),
              );
            }
            setSelectedFolder(normalizeFolderPrefix(nextPath));
          }
        }
        setRenameTarget(null);
        onIndexChange?.();
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "重命名失败");
      }
    },
    [
      onBeforeFilePathChange,
      onFilePathChanged,
      onIndexChange,
      files,
      refresh,
      renameTarget,
    ],
  );

  const handleMove = useCallback(
    async (targetFolder: string) => {
      if (!moveTarget) return;
      try {
        if (moveTarget.kind === "file") {
          const nextPath = resolveMoveFilePath(moveTarget.file, targetFolder);
          if (nextPath !== moveTarget.file.path) {
            await onBeforeFilePathChange?.(moveTarget.file.path);
            await fileRename(moveTarget.file.path, nextPath);
            onFilePathChanged?.(
              moveTarget.file.path,
              nextPath,
              vaultFileTitle(moveTarget.file),
            );
          }
        } else if (moveTarget.kind === "files") {
          const reservedPaths = new Set<string>();
          for (const file of moveTarget.files) {
            const nextPath = resolveMoveFilePath(
              file,
              targetFolder,
              reservedPaths,
            );
            if (nextPath === file.path) continue;
            await onBeforeFilePathChange?.(file.path);
            await fileRename(file.path, nextPath);
            onFilePathChanged?.(file.path, nextPath, vaultFileTitle(file));
            reservedPaths.add(nextPath);
          }
        } else {
          const nextPath = buildFolderPath(
            targetFolder,
            folderNameFromPath(moveTarget.path),
          );
          if (nextPath !== moveTarget.path.replace(/\/$/, "")) {
            const oldPrefix = normalizeFolderPrefix(moveTarget.path);
            const newPrefix = normalizeFolderPrefix(nextPath);
            const movedFiles = files.filter((file) =>
              file.path.startsWith(oldPrefix),
            );
            for (const file of movedFiles) {
              await onBeforeFilePathChange?.(file.path);
            }
            await folderRename(moveTarget.path, nextPath);
            for (const file of movedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              onFilePathChanged?.(
                file.path,
                remappedPath,
                vaultFileTitle(file),
              );
            }
            setSelectedFolder(normalizeFolderPrefix(nextPath));
          }
        }
        setMoveTarget(null);
        onIndexChange?.();
        refresh();
      } catch (e) {
        setError(errorMessage(e, "移动失败"));
      }
    },
    [
      files,
      moveTarget,
      onBeforeFilePathChange,
      onFilePathChanged,
      onIndexChange,
      refresh,
      resolveMoveFilePath,
    ],
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
      for (const file of batchDeleteTarget) {
        await onBeforeFileDelete?.(file.path);
        await fileDelete(file.path);
        onFileDeleted?.(file.path);
      }
      setBatchDeleteTarget([]);
      setSelectedFilePaths(new Set());
      setBatchMode(false);
      onIndexChange?.();
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "批量删除失败");
    }
  }, [
    batchDeleteTarget,
    onBeforeFileDelete,
    onFileDeleted,
    onIndexChange,
    refresh,
  ]);

  const handleFolderDelete = useCallback(async () => {
    if (!folderDeleteTarget) return;
    try {
      await folderDelete(folderDeleteTarget.path);
      setFolderDeleteTarget(null);
      onIndexChange?.();
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "删除文件夹失败");
    }
  }, [folderDeleteTarget, onIndexChange, refresh]);

  const createFromTemplate = async (tmplName: string) => {
    const trimmed = newName.trim();
    const path = allocateAvailableNotePath({
      files: files.filter(isNoteFile),
      folderPrefix: selectedFolder,
      preferredFileName: trimmed || `${DEFAULT_NEW_DOCUMENT_TITLE}.md`,
    });
    await templateCreate(path, tmplName);
    setNewName("");
    onIndexChange?.();
    refresh();
    await onOpen(path, "file-tree");
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
      setCorpora(await corpusList());
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
      <div className="task-overlay-filter flex gap-2 border-b border-border/60 bg-surface-inset/30 px-4 py-2">
        <Input
          className="text-xs"
          value={newName}
          placeholder={`${DEFAULT_NEW_DOCUMENT_TITLE}.md`}
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
              ...(trimmed ? { titleHint: trimmed } : {}),
            });
            const openStartedAt = performance.now();
            const openTraceRequest: PrepareNoteOpenRequest = {
              path: created.path,
              priority: "hot",
              source: "new-note",
              titleHint: created.title,
            };
            const preparedNote = await prepareNoteOpenFromContent(
              openTraceRequest,
              {
                content: created.content,
                isLocked: false,
              },
            );
            setNewName("");
            onIndexChange?.();
            refresh();
            await onOpen(created.path, "file-tree", {
              openBudgetKind: "hot",
              openStartedAt,
              openTraceRequest,
              preparedNote,
              priority: "hot",
              titleHint: created.title,
            });
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
      <div className="task-overlay-results flex min-h-0 flex-1">
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
                    data-testid="corpus-confirm-button"
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
                            : new Set(
                                selectableFolderFiles.map((file) => file.path),
                              ),
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
            <VaultNavigatorLoadingSkeleton />
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
                const isNote = isNoteFile(f);
                const Icon = vaultFileIcon(f);
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
                        aria-label={`选择文档 ${vaultFileTitle(f)}`}
                        checked={selectedFilePaths.has(f.path)}
                        disabled={!isNote}
                        className="h-3.5 w-3.5 shrink-0 rounded border-border"
                        onChange={() => toggleFileSelected(f.path)}
                        onClick={(event) => event.stopPropagation()}
                      />
                    ) : null}
                    <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left hover:text-primary"
                      onFocus={() => {
                        const note = noteListItem(f);
                        if (note) onPrepare?.(note, "file-tree");
                      }}
                      onMouseEnter={() => {
                        const note = noteListItem(f);
                        if (note) onPrepare?.(note, "file-tree");
                      }}
                      onClick={() => {
                        onClose();
                        void Promise.resolve(onOpen(f.path, "file-tree")).catch(
                          () => undefined,
                        );
                      }}
                    >
                      {vaultFileTitle(f)}
                    </button>
                    {isNote ? (
                      <>
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
                          onClick={() =>
                            setMoveTarget({ kind: "file", file: f })
                          }
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
                      </>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

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
        previewPath={(folderPath) => movePreviewPath(moveTarget, folderPath)}
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
          await onBeforeFileDelete?.(deleteTarget.path);
          await fileDelete(deleteTarget.path);
          onFileDeleted?.(deleteTarget.path);
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

export function VaultNavigator(props: VaultNavigatorProps) {
  return (
    <IrisOverlay
      open={props.open}
      onClose={props.onClose}
      title="浏览笔记库"
      size="command"
    >
      <VaultNavigatorBody {...props} />
    </IrisOverlay>
  );
}
