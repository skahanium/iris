import { useCallback, useEffect, useState } from "react";

import { settingsGet, settingsSet } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export const DEFAULT_FEED_AUTO_READ_ENABLED = true;
export const DEFAULT_FEED_BACKGROUND_SYNC_ENABLED = true;
export const DEFAULT_FEED_FETCH_INTERVAL_MINUTES = 60;
const FEED_SETTINGS_CHANGED_EVENT = "iris:feed-settings-changed";

type FeedSettingKey =
  | "feed_auto_read_enabled"
  | "feed_background_sync_enabled"
  | "feed_default_fetch_interval_minutes";

function publishFeedSetting(key: FeedSettingKey, value: boolean | number) {
  window.dispatchEvent(
    new CustomEvent(FEED_SETTINGS_CHANGED_EVENT, { detail: { key, value } }),
  );
}

function clampInterval(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FEED_FETCH_INTERVAL_MINUTES;
  return Math.min(10080, Math.max(15, Math.round(value)));
}

/** RSS 的全局偏好只复用通用 settings 表，不创建专属配置表。 */
export function useFeedSettings() {
  const [autoReadEnabled, setAutoReadEnabledState] = useState(() => {
    const legacy = localStorage.getItem("iris-feed-auto-read");
    return legacy === null
      ? DEFAULT_FEED_AUTO_READ_ENABLED
      : legacy !== "false";
  });
  const [backgroundSyncEnabled, setBackgroundSyncEnabledState] = useState(
    DEFAULT_FEED_BACKGROUND_SYNC_ENABLED,
  );
  const [defaultFetchIntervalMinutes, setDefaultFetchIntervalMinutesState] =
    useState(DEFAULT_FEED_FETCH_INTERVAL_MINUTES);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void Promise.all([
      settingsGet<boolean>("feed_auto_read_enabled"),
      settingsGet<boolean>("feed_background_sync_enabled"),
      settingsGet<number>("feed_default_fetch_interval_minutes"),
    ])
      .then(async ([autoRead, background, interval]) => {
        if (cancelled) return;
        if (typeof autoRead === "boolean") {
          setAutoReadEnabledState(autoRead);
        } else {
          const legacy = localStorage.getItem("iris-feed-auto-read");
          if (legacy !== null) {
            const migrated = legacy !== "false";
            try {
              await settingsSet("feed_auto_read_enabled", migrated);
              if (!cancelled) {
                setAutoReadEnabledState(migrated);
                localStorage.removeItem("iris-feed-auto-read");
              }
            } catch {
              if (!cancelled) setAutoReadEnabledState(migrated);
            }
          }
        }
        if (typeof background === "boolean")
          setBackgroundSyncEnabledState(background);
        if (typeof interval === "number") {
          setDefaultFetchIntervalMinutesState(clampInterval(interval));
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const onChanged = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          key: FeedSettingKey;
          value: boolean | number;
        }>
      ).detail;
      if (!detail) return;
      if (
        detail.key === "feed_auto_read_enabled" &&
        typeof detail.value === "boolean"
      ) {
        setAutoReadEnabledState(detail.value);
      }
      if (
        detail.key === "feed_background_sync_enabled" &&
        typeof detail.value === "boolean"
      ) {
        setBackgroundSyncEnabledState(detail.value);
      }
      if (
        detail.key === "feed_default_fetch_interval_minutes" &&
        typeof detail.value === "number"
      ) {
        setDefaultFetchIntervalMinutesState(clampInterval(detail.value));
      }
    };
    window.addEventListener(FEED_SETTINGS_CHANGED_EVENT, onChanged);
    return () =>
      window.removeEventListener(FEED_SETTINGS_CHANGED_EVENT, onChanged);
  }, []);

  const setAutoReadEnabled = useCallback((enabled: boolean) => {
    setAutoReadEnabledState(enabled);
    publishFeedSetting("feed_auto_read_enabled", enabled);
    if (isTauriRuntime()) {
      void settingsSet("feed_auto_read_enabled", enabled).catch(
        () => undefined,
      );
    }
  }, []);
  const setBackgroundSyncEnabled = useCallback((enabled: boolean) => {
    setBackgroundSyncEnabledState(enabled);
    publishFeedSetting("feed_background_sync_enabled", enabled);
    if (isTauriRuntime()) {
      void settingsSet("feed_background_sync_enabled", enabled).catch(
        () => undefined,
      );
    }
  }, []);
  const setDefaultFetchIntervalMinutes = useCallback((minutes: number) => {
    const next = clampInterval(minutes);
    setDefaultFetchIntervalMinutesState(next);
    publishFeedSetting("feed_default_fetch_interval_minutes", next);
    if (isTauriRuntime()) {
      void settingsSet("feed_default_fetch_interval_minutes", next).catch(
        () => undefined,
      );
    }
  }, []);

  return {
    autoReadEnabled,
    backgroundSyncEnabled,
    defaultFetchIntervalMinutes,
    setAutoReadEnabled,
    setBackgroundSyncEnabled,
    setDefaultFetchIntervalMinutes,
  };
}
