import { useVirtualizer } from "@tanstack/react-virtual";
import {
  BookMarked,
  ChevronRight,
  FileDown,
  FileText,
  Folder,
  FolderPlus,
  Pencil,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Input } from "@/components/ui/input";
import { PromptDialog } from "@/components/common/PromptDialog";
import { TemplateEditor } from "@/components/file/TemplateEditor";
import {
  corpusUpsert,
  exportFile,
  fileDelete,
  fileList,
  fileRead,
  fileRename,
  folderCreate,
  folderDelete,
  folderList,
  folderRename,
  templateCreate,
  templateList,
} from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { displayTitleForFileListItem } from "@/lib/note-display";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
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

function defaultScenesForKind(kind: string): string[] {
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

function TreeFolder({
  node,
  depth,
  selected,
  expanded,
  onSelect,
  onToggle,
  onRename,
  onDelete,
}: {
  node: VaultTreeNode;
  depth: number;
  selected: string;
  expanded: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
  onRename: (path: string) => void;
  onDelete: (path: string, name: string) => void;
}) {
  if (node.kind !== "folder") return null;
  const isOpen = expanded.has(node.path);
  const isSelected = selected === node.path;

  return (
    <div>
      <button
        type="button"
        className={cn(
          "group flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-xs hover:bg-accent",
          isSelected && "bg-accent font-medium text-accent-foreground",
        )}
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
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
        <span className="hidden shrink-0 gap-0.5 group-hover:flex">
          <button
            type="button"
            className="rounded p-0.5 hover:bg-accent-foreground/10"
            title="重命名文件夹"
            onClick={(e) => {
              e.stopPropagation();
              onRename(node.path);
            }}
          >
            <Pencil className="h-3 w-3" />
          </button>
          <button
            type="button"
            className="rounded p-0.5 hover:bg-destructive/10 hover:text-destructive"
            title="删除空文件夹"
            onClick={(e) => {
              e.stopPropagation();
              onDelete(node.path, node.name);
            }}
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </span>
      </button>
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
              onRename={onRename}
              onDelete={onDelete}
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
  const [renameTarget, setRenameTarget] = useState<FileListItem | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<FileListItem | null>(null);
  const [selectedFolder, setSelectedFolder] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [corpusDialogOpen, setCorpusDialogOpen] = useState(false);
  const [corpusKind, setCorpusKind] = useState("regulation");
  const [folderCreateOpen, setFolderCreateOpen] = useState(false);
  const [folderCreateParent, setFolderCreateParent] = useState("");
  const [folderRenameTarget, setFolderRenameTarget] = useState<string | null>(
    null,
  );
  const [folderRenameNewName, setFolderRenameNewName] = useState("");
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

  const virtualizer = useVirtualizer({
    count: folderFiles.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 40,
    overscan: 10,
  });

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

  const toggleExpanded = useCallback((path: string) => {
    setExpanded((prev) => {
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

  const handleFolderRename = useCallback(
    async (newName: string) => {
      const trimmed = newName.trim();
      if (!trimmed || !folderRenameTarget) return;
      const parentPath = folderParentPath(folderRenameTarget);
      const newPath = joinVaultChildPath(parentPath, trimmed);
      try {
        await folderRename(folderRenameTarget, newPath);
        setFolderRenameTarget(null);
        setSelectedFolder(`${newPath.replace(/\\/g, "/")}/`);
        refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : "重命名文件夹失败");
      }
    },
    [folderRenameTarget, refresh],
  );

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

  const handleExportHtml = useCallback(async (path: string) => {
    const md = await fileRead(path);
    const title = path.replace(/\.md$/, "").split("/").pop() ?? "note";
    const html = renderMarkdownWithProfile(md, "vault_preview", {
      context: title,
    }).output;
    const destPath = path.replace(/\.md$/, ".html");
    await exportFile(destPath, html);
  }, []);

  const handleSetCorpus = async (kind: string) => {
    const prefix = selectedFolder.endsWith("/")
      ? selectedFolder
      : `${selectedFolder}/`;
    if (!prefix || prefix === "/") return;
    const normalizedKind = kind.trim() || "general";
    const name =
      prefix.replace(/\/$/, "").split("/").pop() ?? prefix.replace(/\/$/, "");
    await corpusUpsert({
      id: slugFromPath(prefix) || "corpus",
      name,
      pathPrefix: prefix,
      kind: normalizedKind,
      scenes: defaultScenesForKind(normalizedKind),
    });
    setCorpusDialogOpen(false);
  };

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
              setFolderCreateParent("");
              setFolderCreateParent(selectedFolder);
              setFolderCreateOpen(true);
            }}
          >
            <Folder className="h-4 w-4" />
          </Button>
          {selectedFolder && (
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="text-xs"
              title="设为语料库"
              onClick={() => setCorpusDialogOpen(true)}
            >
              <BookMarked className="mr-1 h-3.5 w-3.5" />
              设为语料库
            </Button>
          )}
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
                  onRename={(path) => {
                    const name = path.split("/").filter(Boolean).pop() ?? "";
                    setFolderRenameTarget(path);
                    setFolderRenameNewName(name);
                  }}
                  onDelete={(path, name) => {
                    setFolderDeleteTarget({ path, name });
                  }}
                />
              ) : null,
            )}
          </div>
          <div ref={parentRef} className="min-h-0 flex-1 overflow-auto">
            {loading ? (
              <p className="p-3 text-xs text-muted-foreground">加载中…</p>
            ) : folderFiles.length === 0 ? (
              <p className="p-3 text-xs text-muted-foreground">
                {selectedFolder ? "此文件夹暂无笔记" : "暂无笔记"}
              </p>
            ) : (
              <div
                style={{
                  height: `${virtualizer.getTotalSize()}px`,
                  position: "relative",
                }}
              >
                {virtualizer.getVirtualItems().map((virtualItem) => {
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
                        onClick={() => setRenameTarget(f)}
                      >
                        <Pencil className="h-3 w-3" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        onClick={() => setDeleteTarget(f)}
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        title="导出 HTML"
                        onClick={() => void handleExportHtml(f.path)}
                      >
                        <FileDown className="h-3 w-3" />
                      </Button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </IrisOverlay>

      <PromptDialog
        open={corpusDialogOpen}
        title="设为语料库"
        label="类型（regulation / exemplar / general）"
        defaultValue={corpusKind}
        onCancel={() => setCorpusDialogOpen(false)}
        onSubmit={async (kind) => {
          const k = kind.trim() || "general";
          setCorpusKind(k);
          await handleSetCorpus(k);
        }}
      />

      <PromptDialog
        open={renameTarget !== null}
        title="重命名文件"
        label="新路径"
        defaultValue={renameTarget?.path ?? ""}
        onCancel={() => setRenameTarget(null)}
        onSubmit={async (next) => {
          if (!renameTarget || !next || next === renameTarget.path) {
            setRenameTarget(null);
            return;
          }
          await fileRename(renameTarget.path, next);
          setRenameTarget(null);
          refresh();
        }}
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

      <PromptDialog
        open={folderCreateOpen}
        title="新建文件夹"
        label="文件夹名称"
        defaultValue=""
        onCancel={() => {
          setFolderCreateOpen(false);
          setFolderCreateParent("");
        }}
        onSubmit={handleFolderCreate}
      />

      <PromptDialog
        open={folderRenameTarget !== null}
        title="重命名文件夹"
        label="新名称"
        defaultValue={folderRenameNewName}
        onCancel={() => setFolderRenameTarget(null)}
        onSubmit={handleFolderRename}
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
