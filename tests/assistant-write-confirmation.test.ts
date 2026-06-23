import { describe, expect, it } from "vitest";

import { pendingWriteConfirmationAction } from "@/lib/assistant-write-confirmation";

describe("assistant write confirmation", () => {
  it("applies a single pending patch from explicit confirmation text", () => {
    expect(
      pendingWriteConfirmationAction({
        message: "我确认",
        pendingPatchCount: 1,
      }),
    ).toBe("apply_single_patch");
  });

  it("asks for panel confirmation when multiple patches are pending", () => {
    expect(
      pendingWriteConfirmationAction({
        message: "按此修改",
        pendingPatchCount: 2,
      }),
    ).toBe("clarify_multiple_patches");
  });

  it("ignores confirmation text when no patch is pending", () => {
    expect(
      pendingWriteConfirmationAction({
        message: "是",
        pendingPatchCount: 0,
      }),
    ).toBe("none");
  });

  it("does not treat ordinary note questions as confirmations", () => {
    expect(
      pendingWriteConfirmationAction({
        message: "这个思路是不是过于浅薄了？",
        pendingPatchCount: 1,
      }),
    ).toBe("none");
  });
});
