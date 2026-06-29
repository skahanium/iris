import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { folderParentPath, joinVaultChildPath } from "@/lib/vault-tree";

import {
  availableMoveFolders,
  buildFolderPrefix,
  displayFolderPath,
  fileNameFromPath,
  fileParentPath,
  folderNameFromPath,
  isInvalidLeafName,
  normalizeDocumentName,
  type MoveTarget,
  type RenameTarget,
} from "./vault-navigator-model";

export function FolderCreateDialog({
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

export function RenameItemDialog({
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

export function MoveItemDialog({
  target,
  folders,
  onCancel,
  onSubmit,
  previewPath,
}: {
  target: MoveTarget | null;
  folders: string[];
  onCancel: () => void;
  onSubmit: (folderPath: string) => void;
  previewPath?: (folderPath: string) => string;
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
      ? (previewPath?.(selected) ??
        (target.kind === "file"
          ? joinVaultChildPath(selected, targetName)
          : target.kind === "files"
            ? `${displayFolderPath(selected)} / ${targetName}`
            : buildFolderPrefix(selected, targetName)))
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
