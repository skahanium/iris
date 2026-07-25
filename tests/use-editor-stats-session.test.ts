import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { useEditorStats } from "@/hooks/useEditorStats";

describe("useEditorStats session char delta", () => {
  it("keeps per-session accum when switching active session", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useEditorStats());

    act(() => {
      result.current.setActiveEditorSession("session-a");
      result.current.resetSessionCharDelta("session-a", 100);
      result.current.applySessionCharDelta("session-a", {
        added: 5,
        removed: 0,
      });
    });

    act(() => {
      result.current.setActiveEditorSession("session-b");
      result.current.resetSessionCharDelta("session-b", 200);
      result.current.applySessionCharDelta("session-b", {
        added: 2,
        removed: 1,
      });
    });

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(result.current.editorStats.sessionCharsAdded).toBe(2);
    expect(result.current.editorStats.sessionCharsRemoved).toBe(1);

    act(() => {
      result.current.setActiveEditorSession("session-a");
    });

    expect(result.current.editorStats.sessionCharsAdded).toBe(5);
    expect(result.current.editorStats.sessionCharsRemoved).toBe(0);

    vi.useRealTimers();
  });

  it("hides session delta when character count matches open baseline", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useEditorStats());

    act(() => {
      result.current.setActiveEditorSession("session-a");
      result.current.resetSessionCharDelta("session-a", 10);
      result.current.applySessionCharDelta("session-a", {
        added: 1,
        removed: 0,
      });
      result.current.applySessionCharDelta("session-a", {
        added: 0,
        removed: 1,
      });
      result.current.updateEditorStats({
        characterCount: 10,
        readingMinutes: 1,
      });
    });

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(result.current.editorStats.sessionCharsAdded).toBe(0);
    expect(result.current.editorStats.sessionCharsRemoved).toBe(0);

    vi.useRealTimers();
  });
});
