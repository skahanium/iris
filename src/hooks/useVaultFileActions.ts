import { useCallback, useState } from "react";

import {
  buildFolderPath,
  fileNameFromPath,
  fileParentPath,
  folderNameFromPath,
  isInvalidFolderName,
  normalizeDocumentName,
  normalizeFolderPrefix,
  type MoveTarget,
  type RenameTarget,
} from "@/components/file/vault-navigator-model";
import {
  fileDelete,
  fileRename,
  fileSetLock,
  folderCreate,
  folderRename,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import {
  prepareNoteOpenFromContent,
  type PrepareNoteOpenRequest,
} from "@/lib/note-open-preparation";
import {
  allocateAvailableNotePath,
  isAutoSyncableNotePath,
  isPlaceholderDocumentTitle,
  titleToNotePath,
} from "@/lib/note-names";
import { displayTitleForFileListItem } from "@/lib/note-display";
import { folderParentPath, joinVaultChildPath } from "@/lib/vault-tree";
import type { NoteOpenSource } from "@/lib/document-open-runtime";
import type { FileListItem, FileWriteIndexStatus } from "@/types/ipc";

export interface VaultFileActionCallbacks {
  /** 打开文档（source 由动作传入；新建笔记为 "file-tree"）。 */
  onOpen: (
    path: string,
    source: NoteOpenSource,
    options?: Record<string, unknown>,
  ) => void | Promise<void>;
  /** 路径迁移前（dirty flush + beginPathMigration，由 useNavigatorFileLifecycle 提供）。 */
  onBeforeFilePathChange?: (oldPath: string, newPath: string) => Promise<void>;
  /** 路径迁移完成（tab 替换，由 useNavigatorFileLifecycle 提供）。 */
  onFilePathChanged?: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  /** 路径迁移失败回滚（abortPathMigration）。 */
  onFilePathChangeFailed?: (oldPath: string) => void;
  /** 删除前（dirty flush + discardOpenTab）。 */
  onBeforeFileDelete?: (path: string) => Promise<void>;
  onFileDeleted?: (path: string) => void;
  /** 锁定前（dirty flush，保证锁捕获最新正文）。 */
  onBeforeFileLock?: (path: string) => Promise<void>;
  onFileLockChanged?: (path: string, locked: boolean) => void;
  /** 索引降级弱警示（动作仍视为成功）。 */
  onIndexDegraded?: () => void;
  onIndexChange?: () => void;
  /** catalog 刷新（useVaultCatalog.refresh）。 */
  refresh: () => void;
}

/** 重命名/移动所需的 catalog 上下文（消费方从 useVaultCatalog 传入）。 */
export interface VaultRenameMoveContext {
  files: FileListItem[];
  /** 文件显示标题（媒体文件允许回退到文件名）。 */
  fileTitle: (file: FileListItem) => string;
}

export interface UseVaultFileActionsResult {
  /** 最近一次动作的错误消息（null 表示成功）。 */
  error: string | null;
  clearError: () => void;
  /** 在 folderPrefix 下新建笔记并打开（source: "file-tree"）。 */
  createNote: (options: {
    folderPrefix?: string;
    titleHint?: string;
  }) => Promise<void>;
  /** 新建文件夹；返回创建后的路径，失败返回 null。 */
  createFolder: (parentPath: string, name: string) => Promise<string | null>;
  /** 重命名文件/文件夹；文件夹分支返回新的规范化前缀，失败返回 null。 */
  rename: (
    target: RenameTarget,
    name: string,
    ctx: VaultRenameMoveContext,
  ) => Promise<string | null>;
  /** 移动文件/文件夹（防重名分配 + 批量 reservedPaths）；返回新前缀（文件夹）或 null。 */
  move: (
    target: MoveTarget,
    targetFolder: string,
    ctx: VaultRenameMoveContext,
  ) => Promise<string | null>;
  /** 锁定/解锁（锁定前 flush dirty）。 */
  setLock: (path: string, locked: boolean) => Promise<void>;
  /** 移入回收站。 */
  deleteToRecycleBin: (path: string) => Promise<void>;
}

function asErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  return fallback;
}

/**
 * 共享文件动作 controller（v1.2.19 Task 6）。
 *
 * 固定新建、重命名、移动、锁定与回收站删除的完整链：
 * lifecycle 屏障（dirty flush/路径迁移）→ IPC → 索引回执 → 提交回执 → refresh。
 * 所有 dirty/open-tab 动作必须经由 useNavigatorFileLifecycle 提供的回调。
 */
export function useVaultFileActions(
  callbacks: VaultFileActionCallbacks,
): UseVaultFileActionsResult {
  const [error, setError] = useState<string | null>(null);
  const {
    onOpen,
    onBeforeFilePathChange,
    onFilePathChanged,
    onFilePathChangeFailed,
    onBeforeFileDelete,
    onFileDeleted,
    onBeforeFileLock,
    onFileLockChanged,
    onIndexDegraded,
    onIndexChange,
    refresh,
  } = callbacks;

  const clearError = useCallback(() => setError(null), []);

  const reportIndexStatus = useCallback(
    (status: FileWriteIndexStatus) => {
      if (status === "degraded") onIndexDegraded?.();
    },
    [onIndexDegraded],
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
      files: FileListItem[],
      reservedPaths?: Iterable<string>,
    ) =>
      allocateAvailableNotePath({
        files,
        folderPrefix: targetFolder,
        preferredFileName: preferredMoveFileName(file),
        excludePaths: [file.path],
        reservedPaths,
      }),
    [preferredMoveFileName],
  );

  const createNote = useCallback(
    async (options: { folderPrefix?: string; titleHint?: string }) => {
      const { folderPrefix = "", titleHint } = options;
      try {
        const created = await createDefaultNote({
          folderPrefix,
          ...(titleHint ? { titleHint } : {}),
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
      } catch (e) {
        setError(asErrorMessage(e, "新建笔记失败"));
      }
    },
    [onIndexChange, onOpen, refresh],
  );

  const createFolder = useCallback(
    async (parentPath: string, name: string): Promise<string | null> => {
      const trimmed = name.trim();
      if (!trimmed) return null;
      if (isInvalidFolderName(trimmed)) {
        setError("文件夹名称不能包含路径分隔符或非法字符");
        return null;
      }
      const folderPath = joinVaultChildPath(parentPath, trimmed);
      try {
        await folderCreate(folderPath);
        onIndexChange?.();
        refresh();
        // 返回规范化前缀（含尾斜杠），消费方直接用于选中/展开 key。
        return normalizeFolderPrefix(folderPath);
      } catch (e) {
        setError(asErrorMessage(e, "创建文件夹失败"));
        return null;
      }
    },
    [onIndexChange, refresh],
  );

  const rename = useCallback(
    async (
      target: RenameTarget,
      name: string,
      ctx: VaultRenameMoveContext,
    ): Promise<string | null> => {
      const startedMigrations: string[] = [];
      let folderPrefix: string | null = null;
      try {
        if (target.kind === "file") {
          const parent = fileParentPath(target.file.path);
          const nextPath = joinVaultChildPath(
            parent,
            normalizeDocumentName(name),
          );
          if (nextPath !== target.file.path) {
            await onBeforeFilePathChange?.(target.file.path, nextPath);
            startedMigrations.push(target.file.path);
            const receipt = await fileRename(target.file.path, nextPath);
            reportIndexStatus(receipt.indexStatus);
            onFilePathChanged?.(target.file.path, nextPath, name);
          }
        } else {
          const parent = folderParentPath(target.path);
          const nextPath = buildFolderPath(parent, name);
          if (nextPath !== target.path.replace(/\/$/, "")) {
            const oldPrefix = normalizeFolderPrefix(target.path);
            const newPrefix = normalizeFolderPrefix(nextPath);
            const renamedFiles = ctx.files.filter((file) =>
              file.path.startsWith(oldPrefix),
            );
            for (const file of renamedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              await onBeforeFilePathChange?.(file.path, remappedPath);
              startedMigrations.push(file.path);
            }
            reportIndexStatus(await folderRename(target.path, nextPath));
            for (const file of renamedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              onFilePathChanged?.(file.path, remappedPath, ctx.fileTitle(file));
            }
            folderPrefix = normalizeFolderPrefix(nextPath);
          }
        }
        onIndexChange?.();
        refresh();
      } catch (e) {
        startedMigrations.forEach((oldPath) =>
          onFilePathChangeFailed?.(oldPath),
        );
        setError(asErrorMessage(e, "重命名失败"));
        return null;
      }
      return folderPrefix;
    },
    [
      onBeforeFilePathChange,
      onFilePathChangeFailed,
      onFilePathChanged,
      onIndexChange,
      refresh,
      reportIndexStatus,
    ],
  );

  const move = useCallback(
    async (
      target: MoveTarget,
      targetFolder: string,
      ctx: VaultRenameMoveContext,
    ): Promise<string | null> => {
      const startedMigrations: string[] = [];
      let folderPrefix: string | null = null;
      try {
        if (target.kind === "file") {
          const nextPath = resolveMoveFilePath(
            target.file,
            targetFolder,
            ctx.files,
          );
          if (nextPath !== target.file.path) {
            await onBeforeFilePathChange?.(target.file.path, nextPath);
            startedMigrations.push(target.file.path);
            const receipt = await fileRename(target.file.path, nextPath);
            reportIndexStatus(receipt.indexStatus);
            onFilePathChanged?.(
              target.file.path,
              nextPath,
              ctx.fileTitle(target.file),
            );
          }
        } else if (target.kind === "files") {
          const reservedPaths = new Set<string>();
          for (const file of target.files) {
            const nextPath = resolveMoveFilePath(
              file,
              targetFolder,
              ctx.files,
              reservedPaths,
            );
            if (nextPath === file.path) continue;
            await onBeforeFilePathChange?.(file.path, nextPath);
            startedMigrations.push(file.path);
            const receipt = await fileRename(file.path, nextPath);
            reportIndexStatus(receipt.indexStatus);
            onFilePathChanged?.(file.path, nextPath, ctx.fileTitle(file));
            reservedPaths.add(nextPath);
          }
        } else {
          const nextPath = buildFolderPath(
            targetFolder,
            folderNameFromPath(target.path),
          );
          if (nextPath !== target.path.replace(/\/$/, "")) {
            const oldPrefix = normalizeFolderPrefix(target.path);
            const newPrefix = normalizeFolderPrefix(nextPath);
            const movedFiles = ctx.files.filter((file) =>
              file.path.startsWith(oldPrefix),
            );
            for (const file of movedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              await onBeforeFilePathChange?.(file.path, remappedPath);
              startedMigrations.push(file.path);
            }
            reportIndexStatus(await folderRename(target.path, nextPath));
            for (const file of movedFiles) {
              const remappedPath = joinVaultChildPath(
                newPrefix,
                file.path.slice(oldPrefix.length),
              );
              onFilePathChanged?.(file.path, remappedPath, ctx.fileTitle(file));
            }
            folderPrefix = normalizeFolderPrefix(nextPath);
          }
        }
        onIndexChange?.();
        refresh();
      } catch (e) {
        startedMigrations.forEach((oldPath) =>
          onFilePathChangeFailed?.(oldPath),
        );
        setError(asErrorMessage(e, "移动失败"));
        return null;
      }
      return folderPrefix;
    },
    [
      onBeforeFilePathChange,
      onFilePathChangeFailed,
      onFilePathChanged,
      onIndexChange,
      refresh,
      reportIndexStatus,
      resolveMoveFilePath,
    ],
  );

  const setLock = useCallback(
    async (path: string, locked: boolean) => {
      try {
        await onBeforeFileLock?.(path);
        await fileSetLock(path, locked);
        onFileLockChanged?.(path, locked);
        refresh();
      } catch (e) {
        setError(asErrorMessage(e, "更新锁定状态失败"));
      }
    },
    [onBeforeFileLock, onFileLockChanged, refresh],
  );

  const deleteToRecycleBin = useCallback(
    async (path: string) => {
      try {
        await onBeforeFileDelete?.(path);
        await fileDelete(path);
        onFileDeleted?.(path);
        onIndexChange?.();
        refresh();
      } catch (e) {
        setError(asErrorMessage(e, "删除失败"));
      }
    },
    [onBeforeFileDelete, onFileDeleted, onIndexChange, refresh],
  );

  return {
    error,
    clearError,
    createNote,
    createFolder,
    rename,
    move,
    setLock,
    deleteToRecycleBin,
  };
}
