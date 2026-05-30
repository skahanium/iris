import { describe, expect, it, vi, beforeEach } from "vitest";

import {
  IrisClipboardError,
  readClipboardText,
  writeClipboardText,
  copyTextFieldSelection,
  cutTextFieldSelection,
  pasteIntoTextField,
} from "@/lib/iris-clipboard";

describe("iris-clipboard", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", {
      clipboard: {
        readText: vi.fn(async () => "pasted"),
        writeText: vi.fn(async () => undefined),
      },
    });
  });

  it("readClipboardText returns clipboard content", async () => {
    await expect(readClipboardText()).resolves.toBe("pasted");
  });

  it("writeClipboardText throws IrisClipboardError on failure", async () => {
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(
      new Error("denied"),
    );
    await expect(writeClipboardText("x")).rejects.toBeInstanceOf(
      IrisClipboardError,
    );
  });

  it("copyTextFieldSelection copies slice", async () => {
    const ok = await copyTextFieldSelection("hello", { start: 1, end: 4 });
    expect(ok).toBe(true);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("ell");
  });

  it("cutTextFieldSelection removes selected range", async () => {
    const result = await cutTextFieldSelection("hello", { start: 1, end: 4 });
    expect(result).toEqual({ value: "ho", caret: 1 });
  });

  it("pasteIntoTextField inserts clipboard at selection", async () => {
    const result = await pasteIntoTextField("hi", { start: 2, end: 2 });
    expect(result).toEqual({ value: "hipasted", caret: 8 });
  });
});
