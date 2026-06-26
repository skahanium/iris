import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
      <DialogContent className="w-[min(1040px,calc(100vw-4rem))] max-w-none">
        <DialogHeader className="px-6 pb-2 pr-12 pt-5">
          <DialogTitle>文件冲突：{filePath}</DialogTitle>
        </DialogHeader>
        <p className="px-6 text-sm leading-relaxed text-muted-foreground">
          外部编辑器修改了此笔记。请选择保留哪个版本。
        </p>
        <div className="grid grid-cols-1 gap-4 px-6 py-3 md:grid-cols-2">
          <div>
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              本地版本（编辑器中）
            </div>
            <ScrollArea className="h-[min(46vh,18rem)] rounded-md border border-border bg-muted/25 px-4 py-3 shadow-inner">
              <pre className="whitespace-pre-wrap font-mono text-[13px] leading-6">
                {localContent}
              </pre>
            </ScrollArea>
          </div>
          <div>
            <div className="mb-2 text-xs font-medium text-muted-foreground">
              外部版本
            </div>
            <ScrollArea className="h-[min(46vh,18rem)] rounded-md border border-border bg-muted/25 px-4 py-3 shadow-inner">
              <pre className="whitespace-pre-wrap font-mono text-[13px] leading-6">
                {externalContent}
              </pre>
            </ScrollArea>
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border/60 px-6 py-4">
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
