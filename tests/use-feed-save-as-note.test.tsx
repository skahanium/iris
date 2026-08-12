import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

const { createFeedNote } = vi.hoisted(() => ({
  createFeedNote: vi.fn(),
}));

vi.mock("@/lib/feed-note-export", () => ({ createFeedNote }));

import {
  FeedNoteOpenError,
  useFeedSaveAsNote,
} from "@/hooks/useFeedSaveAsNote";

describe("useFeedSaveAsNote", () => {
  it("文件已创建但打开失败时保留路径，重试只重新打开", async () => {
    createFeedNote.mockResolvedValue("订阅/文章.md");
    const openNote = vi
      .fn()
      .mockRejectedValueOnce(new Error("open failed"))
      .mockResolvedValueOnce(undefined);
    const onOpened = vi.fn();
    const { result } = renderHook(() => useFeedSaveAsNote(openNote, onOpened));

    let firstError: unknown;
    await act(async () => {
      try {
        await result.current(
          "# 正文\n\n> 保存：2026-08-12T01:00:00Z",
          "文章",
          "订阅",
        );
      } catch (error) {
        firstError = error;
      }
    });
    expect(firstError).toBeInstanceOf(FeedNoteOpenError);
    expect((firstError as FeedNoteOpenError).savedPath).toBe("订阅/文章.md");

    await act(async () => {
      await result.current(
        "# 正文\n\n> 保存：2026-08-12T01:00:01Z",
        "文章",
        "订阅",
      );
    });

    expect(createFeedNote).toHaveBeenCalledTimes(1);
    expect(openNote).toHaveBeenCalledTimes(2);
    expect(onOpened).toHaveBeenCalledTimes(1);
  });
});
