import { useCallback, useEffect } from "react";

import { IrisOverlay } from "@/components/ui/iris-overlay";
import { classifiedDisplayName } from "@/lib/classified-path";
import type { ClassifiedStatus } from "@/types/ipc";

import { ClassifiedFileList } from "./ClassifiedFileList";
import { ClassifiedPasswordPrompt } from "./ClassifiedPasswordPrompt";
import { ClassifiedPasswordSetup } from "./ClassifiedPasswordSetup";

interface ClassifiedPanelProps {
  open: boolean;
  onClose: () => void;
  status: ClassifiedStatus;
  waiting: boolean;
  idleDeadline: number | null;
  openClassifiedPaths: string[];
  onOpenFile: (path: string) => void | Promise<void>;
  onPrepareFile?: (path: string, titleHint?: string) => void;
  onUnlockSuccess: () => void;
  onRequestLock: () => Promise<boolean>;
  onActivity: () => void;
  onRefreshStatus: () => Promise<ClassifiedStatus>;
  onEnterWaiting: () => void;
}

export function ClassifiedPanel({
  open,
  onClose,
  status,
  waiting,
  idleDeadline,
  openClassifiedPaths,
  onOpenFile,
  onPrepareFile,
  onUnlockSuccess,
  onRequestLock,
  onActivity,
  onRefreshStatus,
  onEnterWaiting,
}: ClassifiedPanelProps) {
  useEffect(() => {
    if (open) {
      void onRefreshStatus();
    }
  }, [open, onRefreshStatus]);

  const handleClose = useCallback(async () => {
    if ((status === "unlocked" || waiting) && openClassifiedPaths.length > 0) {
      onEnterWaiting();
      onClose();
      return;
    }
    if (status === "unlocked" || waiting) {
      await onRequestLock();
    }
    onClose();
  }, [
    onClose,
    onEnterWaiting,
    onRequestLock,
    openClassifiedPaths.length,
    status,
    waiting,
  ]);

  const handleLock = useCallback(async () => {
    const locked = await onRequestLock();
    if (locked) {
      onClose();
    }
  }, [onClose, onRequestLock]);

  if (!open) return null;

  return (
    <IrisOverlay
      open={open}
      onClose={() => void handleClose()}
      title="涉密保险库"
      size="compact"
      bodyClassName="p-0"
    >
      <div
        className="flex min-h-0 w-full flex-col"
        data-testid="classified-panel"
        onMouseMove={onActivity}
        onKeyDown={onActivity}
      >
        {waiting ? (
          <div className="flex min-h-[22rem] flex-col justify-center gap-4 p-6 text-sm">
            <div className="space-y-1">
              <h3 className="text-lg font-semibold">等待关闭涉密标签页</h3>
              <p className="text-muted-foreground">
                还有 {openClassifiedPaths.length}{" "}
                个涉密标签页未关闭。关闭后保险库会自动锁定。
              </p>
            </div>
            <ul className="grid gap-1.5 text-muted-foreground">
              {openClassifiedPaths.slice(0, 4).map((path) => (
                <li
                  key={path}
                  className="truncate rounded-md border border-border/60 bg-surface-inset/40 px-2 py-1.5"
                >
                  {classifiedDisplayName(path)}
                </li>
              ))}
              {openClassifiedPaths.length > 4 ? (
                <li className="px-2 py-1 text-xs">
                  另有 {openClassifiedPaths.length - 4} 个标签页
                </li>
              ) : null}
            </ul>
            <p className="text-xs text-muted-foreground">
              为避免误关正在编辑的涉密内容，当前不会强制锁定。
            </p>
          </div>
        ) : null}
        {!waiting && status === "needs_setup" ? (
          <ClassifiedPasswordSetup onSuccess={onUnlockSuccess} />
        ) : null}
        {!waiting && status === "locked" ? (
          <ClassifiedPasswordPrompt onSuccess={onUnlockSuccess} />
        ) : null}
        {!waiting && status === "unlocked" ? (
          <ClassifiedFileList
            idleDeadline={idleDeadline}
            onLock={() => void handleLock()}
            onOpenFile={async (path) => {
              onActivity();
              await onOpenFile(path);
              onClose();
            }}
            onPrepareFile={onPrepareFile}
            onActivity={onActivity}
          />
        ) : null}
      </div>
    </IrisOverlay>
  );
}
