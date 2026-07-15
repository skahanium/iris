import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import {
  appUpdateCheck,
  appUpdateDownload,
  appUpdateInstall,
  appUpdatePreflight,
  listenAppUpdateProgress,
  listenAppUpdateStatus,
} from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import type {
  AppUpdateInfo,
  AppUpdatePreflightResult,
  AppUpdateProgressEvent,
  AppUpdateStateEvent,
  AppUpdateStatus,
} from "@/types/ipc";

export interface AppUpdateSnapshot {
  status: AppUpdateStatus;
  info: AppUpdateInfo | null;
  message: string | null;
  progress: AppUpdateProgressEvent | null;
  preflight: AppUpdatePreflightResult | null;
  busy: boolean;
  installBlockedMessage: string | null;
}

export interface AppUpdateController {
  snapshot: AppUpdateSnapshot;
  statusBar: {
    status: AppUpdateStatus;
    info: AppUpdateInfo | null;
  };
  hasUnsaved: boolean;
  check: () => Promise<void>;
  download: () => Promise<void>;
  install: () => Promise<void>;
}

interface UseAppUpdateOptions {
  enabled: boolean;
  hasUnsaved: () => boolean;
  onBlockedInstall?: (message: string) => void;
}

const INITIAL_UPDATE: AppUpdateSnapshot = {
  status: "idle",
  info: null,
  message: null,
  progress: null,
  preflight: null,
  busy: false,
  installBlockedMessage: null,
};

const APP_UPDATE_NETWORK_ERROR_MESSAGE = "无法连接更新服务器，请检查网络后重试";
const APP_UPDATE_DOWNLOAD_ERROR_MESSAGE = "更新下载失败，请稍后重试";
const APP_UPDATE_PREFLIGHT_ERROR_MESSAGE = "兼容性预检失败，请重试";
const APP_UPDATE_INSTALL_ERROR_MESSAGE =
  "更新安装失败，请重试或前往 GitHub Release 手动安装";

function mergeStateEvent(
  current: AppUpdateSnapshot,
  event: AppUpdateStateEvent,
): AppUpdateSnapshot {
  return {
    ...current,
    status: event.status,
    info: event.info ?? current.info,
    message: event.message ?? null,
    busy: event.status === "checking" || event.status === "downloading",
    installBlockedMessage: null,
  };
}

function handleActionError(
  setSnapshot: Dispatch<SetStateAction<AppUpdateSnapshot>>,
  message = APP_UPDATE_NETWORK_ERROR_MESSAGE,
) {
  setSnapshot((current) => ({
    ...current,
    status: "error",
    message,
    busy: false,
    installBlockedMessage: null,
  }));
}

function failedPreflightResult(message: string): AppUpdatePreflightResult {
  return {
    ok: false,
    checks: [
      {
        id: "preflight_ipc_error",
        label: "兼容性预检",
        status: "failed",
        message,
      },
    ],
  };
}

export function useAppUpdate({
  enabled,
  hasUnsaved,
  onBlockedInstall,
}: UseAppUpdateOptions) {
  const [snapshot, setSnapshot] = useState<AppUpdateSnapshot>(INITIAL_UPDATE);

  useEffect(() => {
    if (!enabled || !isTauriRuntime()) return undefined;

    let disposed = false;
    let unlistenStatus: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;

    void listenAppUpdateStatus((event) => {
      if (disposed) return;
      setSnapshot((current) => mergeStateEvent(current, event));
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStatus = fn;
    });

    void listenAppUpdateProgress((event) => {
      if (disposed) return;
      setSnapshot((current) => ({
        ...current,
        progress: event,
        status: event.phase === "finished" ? current.status : "downloading",
        busy: event.phase !== "finished",
      }));
    }).then((fn) => {
      if (disposed) fn();
      else unlistenProgress = fn;
    });

    void appUpdateCheck()
      .then((event) => {
        if (disposed) return;
        setSnapshot((current) => mergeStateEvent(current, event));
      })
      .catch(() => {
        if (disposed) return;
        handleActionError(setSnapshot);
      });

    return () => {
      disposed = true;
      unlistenStatus?.();
      unlistenProgress?.();
    };
  }, [enabled]);

  const check = useCallback(async () => {
    setSnapshot((current) => ({
      ...current,
      status: "checking",
      message: null,
      busy: true,
      installBlockedMessage: null,
    }));
    try {
      const event = await appUpdateCheck();
      setSnapshot((current) => mergeStateEvent(current, event));
    } catch {
      handleActionError(setSnapshot);
    }
  }, []);

  const download = useCallback(async () => {
    setSnapshot((current) => ({
      ...current,
      status: "downloading",
      message: null,
      busy: true,
      progress: null,
      preflight: null,
      installBlockedMessage: null,
    }));
    try {
      const event = await appUpdateDownload();
      setSnapshot((current) => mergeStateEvent(current, event));
      return true;
    } catch {
      handleActionError(setSnapshot, APP_UPDATE_DOWNLOAD_ERROR_MESSAGE);
      return false;
    }
  }, []);

  const preflight = useCallback(async () => {
    try {
      const result = await appUpdatePreflight();
      setSnapshot((current) => ({
        ...current,
        preflight: result,
        status: result.ok ? "ready_to_install" : current.status,
        info: current.info
          ? { ...current.info, preflightPassed: result.ok }
          : current.info,
        busy: false,
        installBlockedMessage: null,
      }));
      return result;
    } catch {
      const result = failedPreflightResult(APP_UPDATE_PREFLIGHT_ERROR_MESSAGE);
      setSnapshot((current) => ({
        ...current,
        preflight: result,
        busy: false,
        installBlockedMessage: null,
      }));
      return result;
    }
  }, []);

  const install = useCallback(async () => {
    if (hasUnsaved()) {
      const message = "安装更新前请先保存所有未保存内容，或取消安装。";
      setSnapshot((current) => ({
        ...current,
        installBlockedMessage: message,
      }));
      onBlockedInstall?.(message);
      return;
    }

    try {
      await appUpdateInstall();
    } catch {
      handleActionError(setSnapshot, APP_UPDATE_INSTALL_ERROR_MESSAGE);
    }
  }, [hasUnsaved, onBlockedInstall]);

  return {
    snapshot,
    check,
    download,
    preflight,
    install,
  };
}

export function useAppUpdateController({
  enabled,
  tabs,
  tabsRef,
  onStatus,
}: {
  enabled: boolean;
  tabs: Array<{ dirty?: boolean }>;
  tabsRef: MutableRefObject<Array<{ dirty?: boolean }>>;
  onStatus: (status: string) => void;
}): AppUpdateController {
  const hasUnsaved = useCallback(
    () => tabsRef.current.some((tab) => tab.dirty),
    [tabsRef],
  );
  const update = useAppUpdate({
    enabled,
    hasUnsaved,
    onBlockedInstall: onStatus,
  });
  const hasUnsavedTabs = tabs.some((tab) => tab.dirty);
  const download = useCallback(async () => {
    const downloaded = await update.download();
    if (!downloaded) return;
    const result = await update.preflight();
    if (!result.ok) onStatus("更新兼容性预检失败，已阻止安装");
  }, [onStatus, update]);

  return useMemo(
    () => ({
      snapshot: update.snapshot,
      statusBar: {
        status: update.snapshot.status,
        info: update.snapshot.info,
      },
      hasUnsaved: hasUnsavedTabs,
      check: update.check,
      download,
      install: update.install,
    }),
    [download, hasUnsavedTabs, update.check, update.install, update.snapshot],
  );
}
