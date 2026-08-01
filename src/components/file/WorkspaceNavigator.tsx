import {
  ArrowDownUp,
  ChevronsDownUp,
  ChevronsUpDown,
  Eye,
  EyeOff,
  FilePlus2,
  FolderPlus,
  Search,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import {
  FolderCreateDialog,
  MoveItemDialog,
  RenameItemDialog,
} from "@/components/file/VaultNavigatorDialogs";
import { WorkspaceNavigatorFileList } from "@/components/file/WorkspaceNavigatorFileList";
import { WorkspaceNavigatorTree } from "@/components/file/WorkspaceNavigatorTree";
import {
  buildFolderPath,
  displayFolderPath,
  fileNameFromPath,
  fileParentPath,
  folderNameFromPath,
  normalizeFolderPrefix,
  type MoveTarget,
  type RenameTarget,
} from "@/components/file/vault-navigator-model";
import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import {
  IrisContextMenu,
  type IrisContextMenuGroup,
} from "@/components/ui/iris-context-menu";
import { Input } from "@/components/ui/input";
import { Tooltip } from "@/components/ui/tooltip";
import { useVaultCatalog, type VaultFileItem } from "@/hooks/useVaultCatalog";
import { useVaultFileActions } from "@/hooks/useVaultFileActions";
import {
  buildFolderTree,
  folderParentPath,
  listDirectFilesInFolder,
  sortFolderTree,
  type FolderSort,
  type FolderTreeNode,
} from "@/lib/vault-tree";
import {
  loadWorkspaceNavigatorPreferences,
  saveWorkspaceNavigatorPreferences,
  type WorkspaceNavigatorPreferences,
} from "@/lib/workspace-navigator-preferences";
import type { FileListItem } from "@/types/ipc";

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
  activePath: string | null;
  onOpenDocument: (path: string, titleHint?: string) => void | Promise<void>;
  onPrepareNote?: (file: FileListItem) => void;
  fileLifecycle: WorkspaceNavigatorFileLifecycle;
}

type FileSort = WorkspaceNavigatorPreferences["fileSort"];
type MenuTarget =
  | { kind: "file"; file: VaultFileItem; x: number; y: number }
  | { kind: "folder"; folder: FolderTreeNode; x: number; y: number }
  | null;

const MENU_ACTIONS = {
  createNote: "新建笔记",
  createFolder: "新建文件夹",
  rename: "重命名",
  move: "移动",
  lock: "锁定",
  unlock: "解锁",
  delete: "移入回收站",
} as const;

function isNoteFile(file: VaultFileItem): boolean {
  return !file.kind || file.kind === "note";
}

function isVisibleMedia(file: VaultFileItem): boolean {
  return file.kind === "media" && file.mediaKind !== null;
}

function visibleFolderPaths(nodes: FolderTreeNode[]): string[] {
  return nodes.flatMap((node) => [
    node.path,
    ...visibleFolderPaths(node.children),
  ]);
}

function folderAncestors(folderPath: string): string[] {
  const paths: string[] = [];
  let current = normalizeFolderPrefix(folderPath);
  while (current) {
    paths.push(current);
    current = folderParentPath(current);
  }
  return paths;
}

function visibleTitle(file: VaultFileItem): string {
  return file.title || fileNameFromPath(file.path);
}

function sortFiles(files: VaultFileItem[], sort: FileSort): VaultFileItem[] {
  const direction = sort.direction === "asc" ? 1 : -1;
  return [...files].sort((left, right) => {
    if (sort.key === "updatedAt") {
      const difference =
        (Date.parse(left.updatedAt) || 0) - (Date.parse(right.updatedAt) || 0);
      if (difference !== 0) return difference * direction;
    } else {
      const difference =
        visibleTitle(left).localeCompare(visibleTitle(right), "zh-Hans-CN") *
        direction;
      if (difference !== 0) return difference;
    }
    return left.path.localeCompare(right.path, "zh-Hans-CN");
  });
}

interface NavigatorIconButtonProps {
  label: string;
  children: ReactNode;
  onClick: () => void;
  pressed?: boolean;
  tooltip?: string;
}

function NavigatorIconButton({
  label,
  children,
  onClick,
  pressed,
  tooltip,
}: NavigatorIconButtonProps) {
  return (
    <Tooltip content={tooltip ?? label} side="bottom">
      <button
        type="button"
        className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 hover:bg-muted/70 hover:text-foreground focus:outline-none motion-reduce:transition-none"
        aria-label={label}
        aria-pressed={pressed}
        onClick={onClick}
      >
        {children}
      </button>
    </Tooltip>
  );
}

/** Obsidian 式上下分层 workspace navigator。 */
export function WorkspaceNavigator({
  activePath,
  onOpenDocument,
  onPrepareNote,
  fileLifecycle,
}: WorkspaceNavigatorProps) {
  const {
    files,
    folders,
    loading,
    error: catalogError,
    refresh,
  } = useVaultCatalog({ watch: true });
  const initialPreferences = useMemo(loadWorkspaceNavigatorPreferences, []);
  const [preferences, setPreferences] =
    useState<WorkspaceNavigatorPreferences>(initialPreferences);
  const [selectedFolder, setSelectedFolder] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState("");
  const [folderSortOpen, setFolderSortOpen] = useState(false);
  const [fileSortOpen, setFileSortOpen] = useState(false);
  const [menuTarget, setMenuTarget] = useState<MenuTarget>(null);
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null);
  const [moveTarget, setMoveTarget] = useState<MoveTarget | null>(null);
  const [folderCreateParent, setFolderCreateParent] = useState("");
  const [folderCreateOpen, setFolderCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);
  const [indexDegraded, setIndexDegraded] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const lastExternalPath = useRef<string | null>(null);

  const folderTree = useMemo(
    () => buildFolderTree(files, folders, isNoteFile),
    [files, folders],
  );
  const sortedFolderTree = useMemo(
    () => sortFolderTree(folderTree, preferences.folderSort),
    [folderTree, preferences.folderSort],
  );
  const allFolderPaths = useMemo(
    () => visibleFolderPaths(folderTree),
    [folderTree],
  );
  const knownFolders = useMemo(
    () => new Set(["", ...allFolderPaths]),
    [allFolderPaths],
  );
  const rootMarkdownCount = useMemo(
    () => listDirectFilesInFolder(files, "").filter(isNoteFile).length,
    [files],
  );

  const directFiles = useMemo(
    () => listDirectFilesInFolder(files, selectedFolder),
    [files, selectedFolder],
  );
  const shownFiles = useMemo(() => {
    const query = searchTerm.trim().toLocaleLowerCase("zh-Hans-CN");
    const filtered = directFiles.filter(
      (file) =>
        (isNoteFile(file) || (preferences.showMedia && isVisibleMedia(file))) &&
        (!query ||
          visibleTitle(file).toLocaleLowerCase("zh-Hans-CN").includes(query) ||
          file.path.toLocaleLowerCase("zh-Hans-CN").includes(query)),
    );
    return sortFiles(filtered, preferences.fileSort);
  }, [directFiles, preferences.fileSort, preferences.showMedia, searchTerm]);

  useEffect(() => {
    saveWorkspaceNavigatorPreferences(preferences);
  }, [preferences]);

  // External document switches intentionally override a prior manual folder selection.
  useEffect(() => {
    if (!activePath || lastExternalPath.current === activePath) return;
    const parent = normalizeFolderPrefix(fileParentPath(activePath));
    if (files.length === 0 || !knownFolders.has(parent)) return;
    lastExternalPath.current = activePath;
    setSelectedFolder(parent);
    setExpanded(
      (previous) => new Set([...previous, ...folderAncestors(parent)]),
    );
  }, [activePath, files.length, knownFolders]);

  // Watcher updates may remove a selected directory; climb to the closest surviving parent.
  useEffect(() => {
    if (knownFolders.has(selectedFolder)) return;
    let fallback = selectedFolder;
    while (fallback && !knownFolders.has(fallback)) {
      fallback = folderParentPath(fallback);
    }
    setSelectedFolder(fallback);
  }, [knownFolders, selectedFolder]);

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

  const selectFolder = useCallback((path: string) => {
    setSelectedFolder(normalizeFolderPrefix(path));
    setSearchTerm("");
    setSearchOpen(false);
  }, []);

  const toggleFolder = useCallback((path: string) => {
    setExpanded((previous) => {
      const next = new Set(previous);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const setFolderSort = useCallback((next: FolderSort) => {
    setPreferences((previous) => ({ ...previous, folderSort: next }));
    setFolderSortOpen(false);
  }, []);
  const setFileSort = useCallback((next: FileSort) => {
    setPreferences((previous) => ({ ...previous, fileSort: next }));
    setFileSortOpen(false);
  }, []);

  const changeDivider = useCallback((next: number) => {
    setPreferences((previous) => ({
      ...previous,
      dividerPercent: Math.min(70, Math.max(25, Math.round(next))),
    }));
  }, []);

  const handleDividerPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const target = event.currentTarget;
      target.setPointerCapture(event.pointerId);
    },
    [],
  );
  const handleDividerPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
      const rect = contentRef.current?.getBoundingClientRect();
      if (!rect || rect.height === 0) return;
      changeDivider(((event.clientY - rect.top) / rect.height) * 100);
    },
    [changeDivider],
  );

  const prepareFile = useCallback(
    (file: VaultFileItem) => {
      // prepared-note 预热的 source 保持 file-tree 默认值。
      onPrepareNote?.({
        path: file.path,
        title: file.title,
        updatedAt: file.updatedAt,
        isLocked: file.isLocked,
      });
    },
    [onPrepareNote],
  );

  const openFile = useCallback(
    (file: VaultFileItem) => {
      void onOpenDocument(file.path, visibleTitle(file));
    },
    [onOpenDocument],
  );

  const fileTitle = useCallback((file: FileListItem) => file.title, []);
  const handleRenameSubmit = useCallback(
    (name: string) => {
      if (!renameTarget) return;
      void fileActions.rename(renameTarget, name, { files, fileTitle });
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
        return `${displayFolderPath(targetFolder)} / ${moveTarget.file.title.trim() || fileNameFromPath(moveTarget.file.path)}`;
      }
      if (moveTarget.kind === "files") {
        return `${displayFolderPath(targetFolder)} / ${moveTarget.files.length} 个文档`;
      }
      return buildFolderPath(targetFolder, folderNameFromPath(moveTarget.path));
    },
    [moveTarget],
  );
  const menuGroups = useMemo<IrisContextMenuGroup[]>(() => {
    if (!menuTarget) return [];
    const items = [
      ...(menuTarget.kind === "folder"
        ? [
            { id: "folder-create-note", label: MENU_ACTIONS.createNote },
            { id: "folder-create-folder", label: MENU_ACTIONS.createFolder },
          ]
        : []),
      { id: "rename", label: MENU_ACTIONS.rename },
      { id: "move", label: MENU_ACTIONS.move },
      ...(menuTarget.kind === "file"
        ? [
            {
              id: "lock-toggle",
              label: menuTarget.file.isLocked
                ? MENU_ACTIONS.unlock
                : MENU_ACTIONS.lock,
            },
            { id: "delete", label: MENU_ACTIONS.delete },
          ]
        : []),
    ];
    return [{ group: "", items }];
  }, [menuTarget]);
  const handleMenuSelect = useCallback(
    (action: string) => {
      if (!menuTarget) return;
      switch (action) {
        case "folder-create-note":
          if (menuTarget.kind === "folder") {
            void fileActions.createNote({
              folderPrefix: menuTarget.folder.path,
            });
          }
          break;
        case "folder-create-folder":
          if (menuTarget.kind === "folder") {
            setFolderCreateParent(menuTarget.folder.path);
            setFolderCreateOpen(true);
          }
          break;
        case "rename":
          setRenameTarget(
            menuTarget.kind === "file"
              ? { kind: "file", file: menuTarget.file }
              : { kind: "folder", path: menuTarget.folder.path },
          );
          break;
        case "move":
          setMoveTarget(
            menuTarget.kind === "file"
              ? { kind: "file", file: menuTarget.file }
              : { kind: "folder", path: menuTarget.folder.path },
          );
          break;
        case "lock-toggle":
          if (menuTarget.kind === "file") {
            void fileActions.setLock(
              menuTarget.file.path,
              !menuTarget.file.isLocked,
            );
          }
          break;
        case "delete":
          if (menuTarget.kind === "file") setDeleteTarget(menuTarget.file);
          break;
        default:
          break;
      }
      setMenuTarget(null);
    },
    [fileActions, menuTarget],
  );

  return (
    <div
      data-testid="workspace-navigator"
      className="relative flex h-full min-h-0 flex-col bg-panel text-xs"
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-border-subtle px-3 py-2">
        <span className="min-w-0 flex-1 text-ui font-medium text-foreground">
          笔记库
        </span>
      </div>

      {indexDegraded ? (
        <p className="shrink-0 border-b border-warning/30 bg-warning-bg px-3 py-1.5 text-[11px] text-warning-foreground">
          索引待修复：文件操作已成功，但搜索索引稍后重建。
        </p>
      ) : null}
      {catalogError ? (
        <p
          data-testid="workspace-navigator-error"
          className="shrink-0 border-b border-destructive/30 px-3 py-2 text-[11px] text-destructive"
        >
          {catalogError}
        </p>
      ) : null}
      {fileActions.error ? (
        <p className="shrink-0 border-b border-destructive/30 px-3 py-2 text-[11px] text-destructive">
          {fileActions.error}
        </p>
      ) : null}

      <div ref={contentRef} className="flex min-h-0 flex-1 flex-col">
        <section
          style={{ height: `${preferences.dividerPercent}%` }}
          className="flex min-h-0 shrink-0 flex-col"
        >
          <div className="relative mx-2 mt-2 flex shrink-0 items-center gap-0.5 rounded-lg border border-border-subtle bg-surface-chrome px-1 py-1">
            <NavigatorIconButton
              label="在当前文件夹新建文件夹"
              onClick={() => {
                setFolderCreateParent(selectedFolder);
                setFolderCreateOpen(true);
              }}
            >
              <FolderPlus className="h-4 w-4" />
            </NavigatorIconButton>
            <NavigatorIconButton
              label="文件夹排序"
              onClick={() => setFolderSortOpen((open) => !open)}
            >
              <ArrowDownUp className="h-4 w-4" />
            </NavigatorIconButton>
            <NavigatorIconButton
              label="展开全部文件夹"
              onClick={() => setExpanded(new Set(allFolderPaths))}
            >
              <ChevronsUpDown className="h-4 w-4" />
            </NavigatorIconButton>
            <NavigatorIconButton
              label="折叠全部文件夹"
              onClick={() => setExpanded(new Set())}
            >
              <ChevronsDownUp className="h-4 w-4" />
            </NavigatorIconButton>
            {folderSortOpen ? (
              <div className="absolute left-8 top-9 z-50 w-40">
                <IrisSurfaceMenuPanel aria-label="文件夹排序">
                  <IrisSurfaceMenuItem
                    id="folder-name-asc"
                    label="名称：升序"
                    onSelect={() =>
                      setFolderSort({ key: "name", direction: "asc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="folder-name-desc"
                    label="名称：降序"
                    onSelect={() =>
                      setFolderSort({ key: "name", direction: "desc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="folder-count-desc"
                    label="直属笔记数：降序"
                    onSelect={() =>
                      setFolderSort({ key: "count", direction: "desc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="folder-count-asc"
                    label="直属笔记数：升序"
                    onSelect={() =>
                      setFolderSort({ key: "count", direction: "asc" })
                    }
                  />
                </IrisSurfaceMenuPanel>
              </div>
            ) : null}
          </div>
          {loading ? (
            <div
              role="status"
              aria-label="笔记库加载中"
              className="flex min-h-0 flex-1 flex-col gap-1.5 px-3 py-2"
            >
              {Array.from({ length: 5 }, (_, index) => (
                <div
                  key={index}
                  className="h-3 animate-pulse rounded bg-muted/50"
                  style={{ width: `${90 - index * 10}%` }}
                />
              ))}
            </div>
          ) : (
            <WorkspaceNavigatorTree
              tree={sortedFolderTree}
              expanded={expanded}
              selectedFolder={selectedFolder}
              rootMarkdownCount={rootMarkdownCount}
              onToggleFolder={toggleFolder}
              onSelectFolder={selectFolder}
              onRowMenu={(folder, _index, x = 8, y = 8) =>
                setMenuTarget({ kind: "folder", folder, x, y })
              }
            />
          )}
        </section>

        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="调整文件夹与文件列表比例"
          aria-valuemin={25}
          aria-valuemax={70}
          aria-valuenow={preferences.dividerPercent}
          tabIndex={0}
          className="group relative h-2 shrink-0 cursor-row-resize touch-none before:absolute before:inset-x-3 before:top-1/2 before:h-px before:bg-border-subtle hover:before:bg-muted-foreground/50"
          onPointerDown={handleDividerPointerDown}
          onPointerMove={handleDividerPointerMove}
          onDoubleClick={() => changeDivider(45)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              changeDivider(preferences.dividerPercent + 5);
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              changeDivider(preferences.dividerPercent - 5);
            }
          }}
        />

        <section className="flex min-h-0 flex-1 flex-col">
          <div className="relative mx-2 flex shrink-0 items-center gap-0.5 rounded-lg border border-border-subtle bg-surface-chrome px-1 py-1">
            {searchOpen ? (
              <>
                <Input
                  aria-label="搜索当前文件夹文件"
                  autoFocus
                  value={searchTerm}
                  onChange={(event) => setSearchTerm(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      event.preventDefault();
                      setSearchTerm("");
                      setSearchOpen(false);
                    }
                  }}
                  className="h-7 min-w-0 flex-1 text-[13px]"
                />
                <NavigatorIconButton
                  label="清空搜索"
                  onClick={() => {
                    setSearchTerm("");
                    setSearchOpen(false);
                  }}
                >
                  <X className="h-4 w-4" />
                </NavigatorIconButton>
              </>
            ) : (
              <NavigatorIconButton
                label="搜索当前文件夹"
                onClick={() => setSearchOpen(true)}
              >
                <Search className="h-4 w-4" />
              </NavigatorIconButton>
            )}
            <NavigatorIconButton
              label={
                preferences.showMedia ? "隐藏直属媒体文件" : "显示直属媒体文件"
              }
              pressed={preferences.showMedia}
              onClick={() =>
                setPreferences((previous) => ({
                  ...previous,
                  showMedia: !previous.showMedia,
                }))
              }
            >
              {preferences.showMedia ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </NavigatorIconButton>
            <NavigatorIconButton
              label="文件排序"
              onClick={() => setFileSortOpen((open) => !open)}
            >
              <ArrowDownUp className="h-4 w-4" />
            </NavigatorIconButton>
            <NavigatorIconButton
              label="在当前文件夹新建笔记"
              onClick={() =>
                void fileActions.createNote({ folderPrefix: selectedFolder })
              }
            >
              <FilePlus2 className="h-4 w-4" />
            </NavigatorIconButton>
            {fileSortOpen ? (
              <div className="absolute right-1 top-9 z-50 w-36">
                <IrisSurfaceMenuPanel aria-label="文件排序">
                  <IrisSurfaceMenuItem
                    id="file-name-asc"
                    label="名称：升序"
                    onSelect={() =>
                      setFileSort({ key: "name", direction: "asc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="file-name-desc"
                    label="名称：降序"
                    onSelect={() =>
                      setFileSort({ key: "name", direction: "desc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="file-updated-desc"
                    label="更新时间：降序"
                    onSelect={() =>
                      setFileSort({ key: "updatedAt", direction: "desc" })
                    }
                  />
                  <IrisSurfaceMenuItem
                    id="file-updated-asc"
                    label="更新时间：升序"
                    onSelect={() =>
                      setFileSort({ key: "updatedAt", direction: "asc" })
                    }
                  />
                </IrisSurfaceMenuPanel>
              </div>
            ) : null}
          </div>
          <div className="flex h-[30px] shrink-0 items-center px-3 text-[14px] font-medium text-foreground">
            {selectedFolder ? folderNameFromPath(selectedFolder) : "根目录"}
          </div>
          <div
            className="min-h-0 flex-1 transition-opacity duration-150 motion-reduce:transition-none"
            key={selectedFolder}
          >
            <WorkspaceNavigatorFileList
              files={shownFiles}
              activePath={activePath}
              onOpenFile={openFile}
              onPrepareFile={prepareFile}
              onRowMenu={(file, _index, x, y) =>
                setMenuTarget({ kind: "file", file, x, y })
              }
            />
          </div>
        </section>
      </div>

      {menuTarget ? (
        <IrisContextMenu
          open
          x={menuTarget.x}
          y={menuTarget.y}
          groups={menuGroups}
          ariaLabel="文件操作"
          onSelect={handleMenuSelect}
          onClose={() => setMenuTarget(null)}
        />
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
              if (created) {
                setExpanded((previous) =>
                  new Set(previous).add(folderCreateParent),
                );
                selectFolder(created);
              }
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
