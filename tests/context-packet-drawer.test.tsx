import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ContextPacketDrawer } from "@/components/ai/ContextPacketDrawer";
import { sessionEvidenceDetail } from "@/lib/ipc";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { ContextPacket } from "@/types/ai";
import type { SessionEvidenceDetailRecord } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  sessionEvidenceDetail: vi.fn(),
}));

const packet: ContextPacket = {
  id: "packet-1",
  source_type: "note",
  source_path: "Draft/Packet.md",
  title: "Packet Source",
  heading_path: "Root > Details",
  source_span: { start: 0, end: 12 },
  content_hash: "packet-hash",
  excerpt: "packet excerpt",
  retrieval_reason: "semantic",
  score: 0.8,
  trust_level: "user_note",
  citation_label: "[C1]",
  stale: false,
};

const ledgerEvidence: SessionEvidenceDetailRecord = {
  id: 1,
  sessionId: 42,
  citationIndex: 1,
  citationLabel: "[C1]",
  sourceType: "local",
  title: "Ledger Source",
  sourcePath: "Ledger/Source.md",
  createdAt: "2026-06-22T00:00:00Z",
};

describe("ContextPacketDrawer evidence detail", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    vi.mocked(sessionEvidenceDetail).mockResolvedValue([ledgerEvidence]);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.clearAllMocks();
  });

  it("opens an evidence source without toggling selection", async () => {
    const onOpenSource = vi.fn();
    const onSelect = vi.fn();

    await act(async () => {
      root.render(
        createElement(ContextPacketDrawer, {
          open: true,
          onOpenChange: vi.fn(),
          packets: [packet],
          selectedIds: [],
          onSelect,
          onOpenSource,
        }),
      );
    });

    act(() => {
      container
        .querySelector("button[title='Open source']")
        ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onOpenSource).toHaveBeenCalledWith(packet);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("opens detail tabs from the session evidence ledger", async () => {
    const onOpenArtifact = vi.fn<(draft: AssistantArtifactDraft) => void>();

    await act(async () => {
      root.render(
        createElement(ContextPacketDrawer, {
          open: true,
          onOpenChange: vi.fn(),
          packets: [packet],
          selectedIds: [],
          onSelect: vi.fn(),
          sessionId: 42,
          onOpenArtifact,
        }),
      );
    });

    await act(async () => {
      container
        .querySelector("button[type='button'][class*='inline-flex']")
        ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(sessionEvidenceDetail).toHaveBeenCalledWith(42);
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "session_evidence_detail",
        persistent: false,
        payload: { sessionId: 42, evidence: [ledgerEvidence] },
      }),
    );
  });

  it("renders compact traceability diagnostics and chain metadata", async () => {
    await act(async () => {
      root.render(
        createElement(ContextPacketDrawer, {
          open: true,
          onOpenChange: vi.fn(),
          packets: [packet],
          selectedIds: [],
          onSelect: vi.fn(),
          relations: [
            {
              sourceId: "packet-1",
              targetId: "packet-1",
              relationType: "supports",
              confidence: 1,
            },
          ],
        }),
      );
    });

    expect(
      container.querySelector("[data-testid='evidence-count-traceable']")
        ?.textContent,
    ).toContain("1 可追溯");
    expect(
      container.querySelector("[data-testid='evidence-chain-meta']")
        ?.textContent,
    ).toContain("Root > Details");
  });
});
