import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";

interface ConflictDialogProps {
  open: boolean;
  localContent: string;
  externalContent: string;
  filePath: string;
  onKeepLocal: () => void;
  onAcceptExternal: () => void;
  onManualEdit: () => void;
}

export function ConflictDialog({
  open,
  localContent,
  externalContent,
  filePath,
  onKeepLocal,
  onAcceptExternal,
  onManualEdit,
}: ConflictDialogProps) {
  return (
    <Dialog open={open} onOpenChange={() => {}}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>文件冲突：{filePath}</DialogTitle>
        </DialogHeader>
        <p className="text-xs text-muted-foreground">
          外部编辑器修改了此笔记。请选择保留哪个版本。
        </p>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">
              本地版本（编辑器中）
            </div>
            <ScrollArea className="h-72 rounded border border-border bg-muted/30 p-2">
              <pre className="whitespace-pre-wrap font-mono text-xs">
                {localContent}
              </pre>
            </ScrollArea>
          </div>
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">
              外部版本
            </div>
            <ScrollArea className="h-72 rounded border border-border bg-muted/30 p-2">
              <pre className="whitespace-pre-wrap font-mono text-xs">
                {externalContent}
              </pre>
            </ScrollArea>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onManualEdit}>
            手动编辑
          </Button>
          <Button type="button" variant="outline" onClick={onAcceptExternal}>
            采用外部
          </Button>
          <Button type="button" onClick={onKeepLocal}>
            保留本地
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
