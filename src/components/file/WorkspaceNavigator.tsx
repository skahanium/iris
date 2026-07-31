import { FilePlus2, FolderPlus } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";

import {
  FolderCreateDialog,
  MoveItemDialog,
  RenameItemDialog,
} from "@/components/file/VaultNavigatorDialogs";
import { WorkspaceNavigatorTree } from "@/components/file/WorkspaceNavigatorTree";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import {
  buildFolderPath,
  displayFolderPath,
  fileNameFromPath,
  folderNameFromPath,
  normalizeFolderPrefix,
  type MoveTarget,
  type RenameTarget,
} from "@/components/file/vault-navigator-model";
import { useVaultCatalog } from "@/hooks/useVaultCatalog";
import { useVaultFileActions } from "@/hooks/useVaultFileActions";
import { buildVaultTree, type VaultTreeNode } from "@/lib/vault-tree";
import type { FileListItem } from "@/types/ipc";

/**
 * 轻量工作区导航的文件生命周期屏障（全部来自 useNavigatorFileLifecycle，
 * 由 App.impl 注入——dirty flush / 路径迁移 / tab 替换不允许绕过）。
 */
export interface WorkspaceNavigatorFileLifecycle {
  handleBeforeFilePathChange: (
    oldPath: string,
    newPath: string,
  ) => Promise<void>;
  handleFilePathChanged: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  handleFilePathChangeFailed: (oldPath: string) => void;
  handleBeforeFileDelete: (path: string) => Promise<void>;
  handleFileDeleted: (path: string) => void;
  handleBeforeFileLock: (path: string) => Promise<void>;
}

interface WorkspaceNavigatorProps {
  /** 当前打开的文档路径（brand marker 与自动显露）。 */
  activePath: string | null;
  /** 打开文档（媒体走 media tab 路由，Markdown 走 prepared-note 管线）。 */
  onOpenDocument: (path: string, titleHint?: string) => void | Promise<void>;
  /** hover/focus 预热（prepareVisibleNote，source 保持 file-tree）。 */
  onPrepareNote?: (file: FileListItem) => void;
  fileLifecycle: WorkspaceNavigatorFileLifecycle;
}

const MENU_ACTIONS = {
  createNote: "新建笔记",
  createFolder: "新建文件夹",
  rename: "重命名",
  move: "移动",
  lock: "锁定",
  unlock: "解锁",
  delete: "移入回收站",
} as const;

/**
 * 轻量 Workspace Navigator（v1.2.19 Task 7）。
 *
 * 消费共享 useVaultCatalog / useVaultFileActions；动作经 IrisSurfaceMenu 提供，
 * 不在每行铺开操作 icon；打开文件不关闭导航器（连续浏览）。
 * 目录展开集合只保存在当前进程（无安全 vault identity 时不持久化绝对路径）。
 */
export function WorkspaceNavigator({
  activePath,
  onOpenDocument,
  onPrepareNote,
  fileLifecycle,
}: WorkspaceNavigatorProps) {
  const catalog = useVaultCatalog({ watch: true });
  const { files, folders, loading, error: catalogError, refresh } = catalog;
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [menuNode, setMenuNode] = useState<VaultTreeNode | null>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(
    null,
  );
  const containerRef = useRef<HTMLDivElement>(null);
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null);
  const [moveTarget, setMoveTarget] = useState<MoveTarget | null>(null);
  const [folderCreateParent, setFolderCreateParent] = useState("");
  const [folderCreateOpen, setFolderCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);
  const [indexDegraded, setIndexDegraded] = useState(false);

  const tree = useMemo(() => buildVaultTree(files, folders), [files, folders]);

  const fileActions = useVaultFileActions({
    onOpen: (path) => onOpenDocument(path),
    onBeforeFilePathChange: fileLifecycle.handleBeforeFilePathChange,
    onFilePathChanged: fileLifecycle.handleFilePathChanged,
    onFilePathChangeFailed: fileLifecycle.handleFilePathChangeFailed,
    onBeforeFileDelete: fileLifecycle.handleBeforeFileDelete,
    onFileDeleted: fileLifecycle.handleFileDeleted,
    onBeforeFileLock: fileLifecycle.handleBeforeFileLock,
    onFileLockChanged: () => undefined,
    onIndexDegraded: () => setIndexDegraded(true),
    refresh,
  });

  const toggleFolder = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const openFile = useCallback(
    (node: VaultTreeNode) => {
      void onOpenDocument(node.path, node.title ?? node.name);
    },
    [onOpenDocument],
  );

  const prepareFile = useCallback(
    (node: VaultTreeNode) => {
      // prepared-note 预热走共享管线，source 保持 "file-tree"。
      onPrepareNote?.({
        path: node.path,
        title: node.name,
        updatedAt: "",
        isLocked: node.locked === true,
      });
    },
    [onPrepareNote],
  );

  const openRowMenu = useCallback((node: VaultTreeNode, rowIndex: number) => {
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const row =
      container.querySelectorAll<HTMLElement>("[role='treeitem']")[rowIndex];
    const rowRect = row?.getBoundingClientRect();
    setMenuPos({
      top: (rowRect?.top ?? rect.top) - rect.top + (rowRect?.height ?? 24) / 2,
      left: 8,
    });
    setMenuNode(node);
  }, []);

  const fileTitle = useCallback((file: FileListItem) => file.title, []);

  const handleRenameSubmit = useCallback(
    (name: string) => {
      if (!renameTarget) return;
      void fileActions.rename(renameTarget, name, {
        files,
        fileTitle,
      });
      setRenameTarget(null);
    },
    [fileActions, fileTitle, files, renameTarget],
  );

  const handleMoveSubmit = useCallback(
    (folderPath: string) => {
      if (!moveTarget) return;
      void fileActions.move(moveTarget, folderPath, { files, fileTitle });
      setMoveTarget(null);
    },
    [fileActions, fileTitle, files, moveTarget],
  );

  const movePreviewPath = useCallback(
    (targetFolder: string): string => {
      if (!moveTarget) return "";
      if (moveTarget.kind === "file") {
        const base =
          moveTarget.file.title.trim() ||
          fileNameFromPath(moveTarget.file.path);
        return `${displayFolderPath(targetFolder)} / ${base}`;
      }
      if (moveTarget.kind === "files") {
        return `${displayFolderPath(targetFolder)} / ${moveTarget.files.length} 个文档`;
      }
      return buildFolderPath(targetFolder, folderNameFromPath(moveTarget.path));
    },
    [moveTarget],
  );

  const menuIsFolder = menuNode?.kind === "folder";
  const menuFile = menuNode && menuNode.kind === "file" ? menuNode : null;

  return (
    <div
      ref={containerRef}
      data-testid="workspace-navigator"
      className="flex h-full min-h-0 flex-col bg-panel text-xs"
    >
      <div className="flex shrink-0 items-center justify-between border-b border-border-subtle px-3 py-1.5">
        <span className="text-xs font-medium text-foreground">笔记库</span>
        <span className="text-micro text-muted-foreground">Ctrl/Cmd+\</span>
      </div>
      {indexDegraded ? (
        <p className="shrink-0 border-b border-warning/30 bg-warning-bg px-3 py-1.5 text-[11px] text-warning-foreground">
          索引待修复：文件操作已成功，但搜索索引稍后重建。
        </p>
      ) : null}{" "}
      {catalogError ? (
        <p
          className="shrink-0 border-b border-destructive/30 px-3 py-2 text-[11px] text-destructive"
          data-testid="workspace-navigator-error"
        >
          {catalogError}
        </p>
      ) : null}
      {fileActions.error ? (
        <p className="shrink-0 border-b border-destructive/30 px-3 py-2 text-[11px] text-destructive">
          {fileActions.error}
        </p>
      ) : null}
      {loading ? (
        <div
          role="status"
          aria-label="笔记库加载中"
          className="flex min-h-0 flex-1 flex-col gap-1.5 px-3 py-2"
        >
          {Array.from({ length: 6 }, (_, index) => (
            <div
              key={index}
              className="h-3 animate-pulse rounded bg-muted/50"
              style={{ width: `${92 - index * 9}%` }}
            />
          ))}
        </div>
      ) : (
        <WorkspaceNavigatorTree
          tree={tree}
          expanded={expanded}
          activePath={activePath}
          onToggleFolder={toggleFolder}
          onOpenFile={openFile}
          onPrepareFile={prepareFile}
          onRowMenu={openRowMenu}
        />
      )}
      {menuNode ? (
        <div
          className="absolute z-50"
          style={{
            top: menuPos?.top ?? 0,
            left: menuPos?.left ?? 0,
          }}
          onMouseLeave={() => setMenuNode(null)}
        >
          <IrisSurfaceMenuPanel aria-label="文件操作">
            {menuIsFolder ? (
              <>
                <IrisSurfaceMenuItem
                  id="create-note"
                  label={MENU_ACTIONS.createNote}
                  icon={<FilePlus2 className="h-4 w-4" />}
                  onSelect={() => {
                    setFolderCreateParent(menuNode.path);
                    setFolderCreateOpen(true);
                    setMenuNode(null);
                  }}
                />
                <IrisSurfaceMenuItem
                  id="create-folder"
                  label={MENU_ACTIONS.createFolder}
                  icon={<FolderPlus className="h-4 w-4" />}
                  onSelect={() => {
                    setFolderCreateParent(menuNode.path);
                    setFolderCreateOpen(true);
                    setMenuNode(null);
                  }}
                />
              </>
            ) : null}
            <IrisSurfaceMenuItem
              id="rename"
              label={MENU_ACTIONS.rename}
              onSelect={() => {
                if (menuFile) {
                  setRenameTarget({
                    kind: "file",
                    file: {
                      path: menuFile.path,
                      title: menuFile.title ?? menuFile.name,
                      updatedAt: "",
                      isLocked: menuFile.locked === true,
                    },
                  });
                } else if (menuNode) {
                  setRenameTarget({
                    kind: "folder",
                    path: normalizeFolderPrefix(menuNode.path),
                  });
                }
                setMenuNode(null);
              }}
            />
            <IrisSurfaceMenuItem
              id="move"
              label={MENU_ACTIONS.move}
              onSelect={() => {
                if (menuFile) {
                  setMoveTarget({
                    kind: "file",
                    file: {
                      path: menuFile.path,
                      title: menuFile.title ?? menuFile.name,
                      updatedAt: "",
                      isLocked: menuFile.locked === true,
                    },
                  });
                } else if (menuNode) {
                  setMoveTarget({
                    kind: "folder",
                    path: normalizeFolderPrefix(menuNode.path),
                  });
                }
                setMenuNode(null);
              }}
            />
            {menuFile ? (
              <IrisSurfaceMenuItem
                id="lock-toggle"
                label={
                  menuFile.locked ? MENU_ACTIONS.unlock : MENU_ACTIONS.lock
                }
                onSelect={() => {
                  void fileActions.setLock(menuFile.path, !menuFile.locked);
                  setMenuNode(null);
                }}
              />
            ) : null}
            {menuFile ? (
              <IrisSurfaceMenuItem
                id="delete"
                label={MENU_ACTIONS.delete}
                onSelect={() => {
                  setDeleteTarget({
                    path: menuFile.path,
                    title: menuFile.title ?? menuFile.name,
                    updatedAt: "",
                    isLocked: menuFile.locked === true,
                  });
                  setMenuNode(null);
                }}
              />
            ) : null}
          </IrisSurfaceMenuPanel>
        </div>
      ) : null}
      <FolderCreateDialog
        open={folderCreateOpen}
        parentPath={folderCreateParent}
        onCancel={() => {
          setFolderCreateOpen(false);
          setFolderCreateParent("");
        }}
        onSubmit={(name) => {
          void fileActions
            .createFolder(folderCreateParent, name)
            .then((created) => {
              setFolderCreateOpen(false);
              setFolderCreateParent("");
              if (created) setExpanded((prev) => new Set(prev).add(created));
            });
        }}
      />
      <RenameItemDialog
        target={renameTarget}
        onCancel={() => setRenameTarget(null)}
        onSubmit={handleRenameSubmit}
      />
      <MoveItemDialog
        target={moveTarget}
        folders={folders}
        onCancel={() => setMoveTarget(null)}
        onSubmit={handleMoveSubmit}
        previewPath={movePreviewPath}
      />
      <ConfirmDialog
        open={deleteTarget !== null}
        title="移入回收站"
        message={`确定将「${deleteTarget?.title ?? ""}」移入回收站？`}
        description="正文、时间线快照与定稿将一并移入回收站，15 天内可恢复。"
        confirmLabel="移入回收站"
        variant="destructive"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (!deleteTarget) return;
          const path = deleteTarget.path;
          setDeleteTarget(null);
          void fileActions.deleteToRecycleBin(path);
        }}
      />
    </div>
  );
}
