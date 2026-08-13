import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { settingsGet, settingsSet } = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({ settingsGet, settingsSet }));
vi.mock("@/lib/tauri-runtime", () => ({ isTauriRuntime: () => true }));

import {
  DEFAULT_FEED_AUTO_READ_ENABLED,
  DEFAULT_FEED_BACKGROUND_SYNC_ENABLED,
  DEFAULT_FEED_FETCH_INTERVAL_MINUTES,
  useFeedSettings,
} from "@/hooks/useFeedSettings";

describe("useFeedSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    settingsGet.mockResolvedValue(null);
    settingsSet.mockResolvedValue(undefined);
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("migrates the legacy auto-read preference only when settings has no value", async () => {
    localStorage.setItem("iris-feed-auto-read", "false");
    settingsGet
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(180);

    const { result } = renderHook(() => useFeedSettings());

    await waitFor(() =>
      expect(settingsSet).toHaveBeenCalledWith("feed_auto_read_enabled", false),
    );
    expect(result.current.autoReadEnabled).toBe(false);
    expect(result.current.backgroundSyncEnabled).toBe(true);
    expect(result.current.defaultFetchIntervalMinutes).toBe(180);
    expect(localStorage.getItem("iris-feed-auto-read")).toBeNull();
  });

  it("does not overwrite a persisted auto-read preference with the legacy value", async () => {
    localStorage.setItem("iris-feed-auto-read", "false");
    settingsGet
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(null);

    const { result } = renderHook(() => useFeedSettings());

    await waitFor(() => expect(result.current.autoReadEnabled).toBe(true));
    expect(settingsSet).not.toHaveBeenCalled();
    expect(localStorage.getItem("iris-feed-auto-read")).toBe("false");
  });

  it("uses safe defaults and persists bounded setting updates", async () => {
    const { result } = renderHook(() => useFeedSettings());
    await waitFor(() => expect(settingsGet).toHaveBeenCalledTimes(3));
    expect(result.current.autoReadEnabled).toBe(DEFAULT_FEED_AUTO_READ_ENABLED);
    expect(result.current.backgroundSyncEnabled).toBe(
      DEFAULT_FEED_BACKGROUND_SYNC_ENABLED,
    );
    expect(result.current.defaultFetchIntervalMinutes).toBe(
      DEFAULT_FEED_FETCH_INTERVAL_MINUTES,
    );

    act(() => result.current.setDefaultFetchIntervalMinutes(1));
    expect(result.current.defaultFetchIntervalMinutes).toBe(15);
    expect(settingsSet).toHaveBeenLastCalledWith(
      "feed_default_fetch_interval_minutes",
      15,
    );
  });
});
