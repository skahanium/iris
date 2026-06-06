import { useCallback, useEffect } from "react";

import { IrisOverlay } from "@/components/ui/iris-overlay";
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
  onOpenFile: (path: string) => void;
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
        className="flex min-h-0 w-full max-w-md flex-col"
        data-testid="classified-panel"
        onMouseMove={onActivity}
        onKeyDown={onActivity}
      >
        {waiting ? (
          <div className="flex flex-col gap-3 p-4 text-sm">
            <h3 className="text-lg font-semibold">等待涉密文件关闭</h3>
            <p className="text-muted-foreground">
              涉密保险库将在所有涉密笔记标签关闭后自动锁定。请先关闭编辑器中的涉密文件。
            </p>
            <ul className="list-disc pl-5 text-muted-foreground">
              {openClassifiedPaths.map((path) => (
                <li key={path} className="truncate">
                  {path}
                </li>
              ))}
            </ul>
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
            onOpenFile={(path) => {
              onActivity();
              onOpenFile(path);
              onClose();
            }}
            onActivity={onActivity}
          />
        ) : null}
      </div>
    </IrisOverlay>
  );
}
