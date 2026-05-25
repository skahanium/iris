import { FileDown, FolderPlus, Pencil, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  exportFile,
  fileCreate,
  fileDelete,
  fileList,
  fileRead,
  fileRename,
  templateCreate,
  templateList,
} from "@/lib/ipc";
import { markdownToHtmlPage } from "@/lib/markdown";
import type { FileListItem } from "@/types/ipc";

interface FileSheetProps {
  open: boolean;
  onClose: () => void;
  onOpen: (path: string) => void;
}

export function FileSheet({ open, onClose, onOpen }: FileSheetProps) {
  const [files, setFiles] = useState<FileListItem[]>([]);
  const [newName, setNewName] = useState("新笔记.md");
  const [templates, setTemplates] = useState<{ name: string }[]>([]);
  const [showTemplates, setShowTemplates] = useState(false);

  const refresh = () => void fileList().then(setFiles);

  useEffect(() => {
    if (open) {
      refresh();
      void templateList().then(setTemplates);
      setShowTemplates(false);
    }
  }, [open]);

  const createFromTemplate = async (tmplName: string) => {
    await templateCreate(newName, tmplName);
    refresh();
    onOpen(newName);
    setShowTemplates(false);
  };

  const handleExportHtml = useCallback(async (path: string) => {
    const md = await fileRead(path);
    const title = path.replace(/\.md$/, "").split("/").pop() ?? "note";
    const html = markdownToHtmlPage(md, title);
    const destPath = path.replace(/\.md$/, ".html");
    await exportFile(destPath, html);
  }, []);

  if (!open) return null;

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-80 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">文件</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>
      <div className="flex gap-2 border-b border-border p-2">
        <Input
          className="text-xs"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
        />
        <Button
          type="button"
          size="icon"
          variant="outline"
          title="新建"
          onClick={async () => {
            if (showTemplates && templates.length > 0) return;
            await fileCreate(newName);
            refresh();
            onOpen(newName);
          }}
        >
          <FolderPlus className="h-4 w-4" />
        </Button>
      </div>
      {templates.length > 0 && (
        <div className="border-b border-border px-2 pb-2">
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
                <Button
                  key={t.name}
                  type="button"
                  size="sm"
                  variant="outline"
                  className="text-xs"
                  onClick={() => void createFromTemplate(t.name)}
                >
                  {t.name}
                </Button>
              ))}
            </div>
          )}
        </div>
      )}
      <ScrollArea className="flex-1">
        {files.map((f) => (
          <div
            key={f.path}
            className="flex items-center gap-1 border-b border-border/50 px-2 py-1.5 text-sm"
          >
            <button
              type="button"
              className="min-w-0 flex-1 truncate text-left hover:text-primary"
              onClick={() => {
                onOpen(f.path);
                onClose();
              }}
            >
              {f.title}
            </button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              onClick={async () => {
                const next = prompt("新路径", f.path);
                if (next) {
                  await fileRename(f.path, next);
                  refresh();
                }
              }}
            >
              <Pencil className="h-3 w-3" />
            </Button>
            <Button
              type="button"
              size="icon"
              variant="ghost"
              onClick={async () => {
                if (confirm(`删除 ${f.path}？`)) {
                  await fileDelete(f.path);
                  refresh();
                }
              }}
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
        ))}
      </ScrollArea>
    </div>
  );
}
