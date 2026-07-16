import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DocumentRecoveryDialog } from "@/components/file/DocumentRecoveryDialog";
import type { DocumentRecoveryAudit } from "@/types/ipc";

const documentRecoveryAudit = vi.fn();
const documentRecoveryRestoreMissing = vi.fn();
const documentRecoveryRestoreOrphan = vi.fn();
const documentTitleRepair = vi.fn();

vi.mock("@/lib/ipc", () => ({
  documentRecoveryAudit: (...args: unknown[]) => documentRecoveryAudit(...args),
  documentRecoveryRestoreMissing: (...args: unknown[]) =>
    documentRecoveryRestoreMissing(...args),
  documentRecoveryRestoreOrphan: (...args: unknown[]) =>
    documentRecoveryRestoreOrphan(...args),
  documentTitleRepair: (...args: unknown[]) => documentTitleRepair(...args),
}));

const audit: DocumentRecoveryAudit = {
  titleIssues: [],
  missingDocuments: [
    {
      path: "notes/missing.md",
      currentTitle: "Missing",
      candidateTitle: "Recovered title",
      versionId: 12,
      contentHash: "a".repeat(64),
      createdAt: "2026-07-16T00:00:00Z",
      preview: "Recovered body preview",
    },
  ],
  orphanedDocuments: [
    {
      objectHash: "b".repeat(64),
      candidateTitle: "Orphan title",
      suggestedPath: "Recovered/default.md",
      preview: "Orphan body preview",
    },
  ],
  unavailableDocuments: [
    {
      path: "notes/unavailable.md",
      currentTitle: "Unavailable",
      reason: "no_readable_version_snapshot",
    },
  ],
};

describe("DocumentRecoveryDialog", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    documentRecoveryAudit.mockReset();
    documentRecoveryRestoreMissing.mockReset();
    documentRecoveryRestoreOrphan.mockReset();
    documentTitleRepair.mockReset();
    documentRecoveryAudit.mockResolvedValue(audit);
    documentRecoveryRestoreMissing.mockResolvedValue({});
    documentRecoveryRestoreOrphan.mockResolvedValue({});
    vi.spyOn(window, "confirm").mockReturnValue(true);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("previews each recoverable source and requires an explicit recovery action", async () => {
    const onRecovered = vi.fn();
    await act(async () => {
      root.render(
        <DocumentRecoveryDialog
          open
          onClose={() => {}}
          onRecovered={onRecovered}
        />,
      );
      await Promise.resolve();
    });

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain("Recovered body preview");
      expect(document.body.textContent).toContain("Orphan body preview");
      expect(document.body.textContent).toContain("没有可验证的本地版本快照");
    });

    const restoreMissing = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent === "恢复到原路径",
    );
    expect(restoreMissing).toBeTruthy();
    await act(async () => {
      restoreMissing?.click();
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(documentRecoveryRestoreMissing).toHaveBeenCalledWith(
        "notes/missing.md",
        12,
        "a".repeat(64),
      );
    });

    const target = document.querySelector(
      `input[aria-label="恢复路径 ${"b".repeat(64)}"]`,
    ) as HTMLInputElement;
    expect(target.value).toBe("Recovered/default.md");
  });
});
