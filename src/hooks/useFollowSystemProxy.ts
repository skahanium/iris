import { useCallback, useEffect, useState } from "react";

import { networkProxyStatus, settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export const DEFAULT_FOLLOW_SYSTEM_PROXY = true;
export const DEFAULT_PROXY_STATUS_LABEL = "无代理";

export function useFollowSystemProxy() {
  const [followSystemProxy, setFollowSystemProxyState] = useState(
    DEFAULT_FOLLOW_SYSTEM_PROXY,
  );
  const [proxyStatusLabel, setProxyStatusLabel] = useState(
    DEFAULT_PROXY_STATUS_LABEL,
  );

  const refreshProxyStatus = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const status = await networkProxyStatus();
      setFollowSystemProxyState(status.follow);
      setProxyStatusLabel(status.label || DEFAULT_PROXY_STATUS_LABEL);
    } catch {
      // Keep the last known label.
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void (async () => {
      try {
        const [enabled, status] = await Promise.all([
          settingsGet<boolean>("follow_system_proxy"),
          networkProxyStatus(),
        ]);
        if (cancelled) return;
        if (typeof enabled === "boolean") {
          setFollowSystemProxyState(enabled);
        } else {
          setFollowSystemProxyState(status.follow);
        }
        setProxyStatusLabel(status.label || DEFAULT_PROXY_STATUS_LABEL);
      } catch {
        if (!cancelled) {
          setProxyStatusLabel(DEFAULT_PROXY_STATUS_LABEL);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const setFollowSystemProxy = useCallback((enabled: boolean) => {
    setFollowSystemProxyState(enabled);
    if (!enabled) {
      setProxyStatusLabel(DEFAULT_PROXY_STATUS_LABEL);
    }
    if (!isTauriRuntime()) return;
    void (async () => {
      await settingsSet("follow_system_proxy", enabled);
      try {
        const status = await networkProxyStatus();
        setFollowSystemProxyState(status.follow);
        setProxyStatusLabel(status.label || DEFAULT_PROXY_STATUS_LABEL);
      } catch {
        if (!enabled) {
          setProxyStatusLabel(DEFAULT_PROXY_STATUS_LABEL);
        }
      }
    })();
  }, []);

  return {
    followSystemProxy,
    proxyStatusLabel,
    setFollowSystemProxy,
    refreshProxyStatus,
  };
}
