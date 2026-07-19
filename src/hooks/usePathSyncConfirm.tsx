import { useCallback, useRef, useState } from "react";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";

export function usePathSyncConfirm() {
  const pathSyncConfirmRef = useRef<{
    message: string;
    resolve: (accepted: boolean) => void;
  } | null>(null);
  const [pathSyncConfirmOpen, setPathSyncConfirmOpen] = useState(false);
  const [pathSyncConfirmMessage, setPathSyncConfirmMessage] = useState("");

  const confirmPathSync = useCallback((message: string) => {
    return new Promise<boolean>((resolve) => {
      pathSyncConfirmRef.current = { message, resolve };
      setPathSyncConfirmMessage(message);
      setPathSyncConfirmOpen(true);
    });
  }, []);

  const finishPathSyncConfirm = useCallback((accepted: boolean) => {
    pathSyncConfirmRef.current?.resolve(accepted);
    pathSyncConfirmRef.current = null;
    setPathSyncConfirmOpen(false);
  }, []);

  const pathSyncConfirmDialog = (
    <ConfirmDialog
      open={pathSyncConfirmOpen}
      title="同步文件路径"
      message={pathSyncConfirmMessage}
      confirmLabel="同步"
      cancelLabel="保留当前路径"
      confirmTestId="path-sync-confirm"
      cancelTestId="path-sync-cancel"
      onConfirm={() => finishPathSyncConfirm(true)}
      onCancel={() => finishPathSyncConfirm(false)}
    />
  );

  return { confirmPathSync, pathSyncConfirmDialog };
}
