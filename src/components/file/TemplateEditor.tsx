import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Textarea } from "@/components/ui/textarea";
import { templateDelete, templateRead, templateSave } from "@/lib/ipc";

interface TemplateEditorProps {
  open: boolean;
  templateName: string | null;
  onClose: () => void;
  onSaved: () => void;
}

export function TemplateEditor({
  open,
  templateName,
  onClose,
  onSaved,
}: TemplateEditorProps) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  useEffect(() => {
    if (open && templateName) {
      setLoading(true);
      setError(null);
      templateRead(templateName)
        .then(setContent)
        .catch((e) => setError(e instanceof Error ? e.message : "加载模板失败"))
        .finally(() => setLoading(false));
    }
  }, [open, templateName]);

  const handleSave = useCallback(async () => {
    if (!templateName) return;
    setLoading(true);
    setError(null);
    try {
      await templateSave(templateName, content);
      onSaved();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "保存模板失败");
    } finally {
      setLoading(false);
    }
  }, [templateName, content, onSaved, onClose]);

  const handleDelete = useCallback(async () => {
    if (!templateName) return;
    setLoading(true);
    setError(null);
    try {
      await templateDelete(templateName);
      onSaved();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "删除模板失败");
    } finally {
      setLoading(false);
    }
  }, [templateName, onSaved, onClose]);

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title={templateName ? `编辑模板: ${templateName}` : "编辑模板"}
        size="command"
      >
        <div className="flex flex-col gap-4 p-4">
          {error && <p className="text-xs text-destructive">{error}</p>}
          {loading ? (
            <p className="text-xs text-muted-foreground">加载中…</p>
          ) : (
            <Textarea
              className="min-h-[300px] font-mono text-sm"
              value={content}
              onChange={(e) => setContent(e.target.value)}
              placeholder="输入模板内容（Markdown）"
            />
          )}
          <div className="flex justify-between">
            <Button
              type="button"
              variant="destructive"
              size="sm"
              onClick={() => setDeleteConfirmOpen(true)}
              disabled={loading}
            >
              删除模板
            </Button>
            <div className="flex gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onClose}
                disabled={loading}
              >
                取消
              </Button>
              <Button
                type="button"
                size="sm"
                onClick={handleSave}
                disabled={loading}
              >
                保存
              </Button>
            </div>
          </div>
        </div>
      </IrisOverlay>

      <ConfirmDialog
        open={deleteConfirmOpen}
        title="删除模板"
        message={`确定删除模板「${templateName ?? ""}」？`}
        description="此操作不可撤销。"
        confirmLabel="删除"
        variant="destructive"
        onCancel={() => setDeleteConfirmOpen(false)}
        onConfirm={handleDelete}
      />
    </>
  );
}
