import { describe, expect, it } from "vitest";

import { DOCUMENT_OPEN_BUDGETS } from "../src/lib/document-open-runtime";

describe("document open performance budgets", () => {
  it("keeps hot and warm visible-open budgets tight", () => {
    expect(DOCUMENT_OPEN_BUDGETS.hotTabCommitMs).toBeLessThanOrEqual(16);
    expect(DOCUMENT_OPEN_BUDGETS.warmPreparedCommitMs).toBeLessThanOrEqual(50);
  });

  it("keeps cold-open feedback fast enough to feel responsive", () => {
    expect(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs).toBeLessThanOrEqual(100);
    expect(DOCUMENT_OPEN_BUDGETS.coldFirstEditorFrameMs).toBeLessThanOrEqual(
      1000,
    );
  });
});
