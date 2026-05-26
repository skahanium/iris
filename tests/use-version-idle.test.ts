import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

import {
  useVersionIdle,
  VERSION_IDLE_MS,
} from "@/hooks/useVersionIdle";

const versionSaveIdle = vi.fn().mockResolvedValue(null);

vi.mock("@/lib/ipc", () => ({
  versionSaveIdle: (...args: unknown[]) => versionSaveIdle(...args),
}));

function TestHarness({
  path,
  content,
  onReady,
}: {
  path: string | null;
  content: string;
  onReady: (api: { onActivity: () => void }) => void;
}) {
  const { onActivity } = useVersionIdle(path, () => content);
  onReady({ onActivity });
  return null;
}

describe("useVersionIdle", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    versionSaveIdle.mockClear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("fires versionSaveIdle after idle interval", async () => {
    let onActivity!: () => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          content: "body",
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS);
    });

    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("note.md", "body");
  });

  it("resets idle timer on activity", async () => {
    let onActivity!: () => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          path: "note.md",
          content: "body",
          onReady: (api) => {
            onActivity = api.onActivity;
          },
        }),
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS / 2);
    });
    expect(versionSaveIdle).not.toHaveBeenCalled();

    act(() => {
      onActivity();
    });

    await act(async () => {
      vi.advanceTimersByTime(VERSION_IDLE_MS - 1000);
    });
    expect(versionSaveIdle).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
  });
});
