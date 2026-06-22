import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { EvidenceDetailArtifactView } from "@/components/ai/EvidenceDetailArtifact";
import type { SessionEvidenceRecord } from "@/types/ipc";

const localEvidence: SessionEvidenceRecord = {
  id: 1,
  sessionId: 10,
  citationIndex: 1,
  citationLabel: "[C1]",
  packetKey: "local:key",
  messageSeqFirst: 2,
  sourceType: "local",
  title: "Local Case",
  sourcePath: "Cases/Alpha.md",
  sourceSpanStart: 0,
  sourceSpanEnd: 12,
  headingPath: "Intro > Facts",
  contentHash: "hash-a",
  retrievalReason: "semantic",
  score: 0.9,
  confidence: "high",
  createdAt: "2026-06-22T00:00:00Z",
};

const webEvidence: SessionEvidenceRecord = {
  id: 2,
  sessionId: 10,
  citationIndex: 2,
  citationLabel: "[C2]",
  packetKey: "web:key",
  messageSeqFirst: 2,
  sourceType: "web",
  title: "Official Web",
  url: "https://example.com/report",
  domain: "example.com",
  retrievalReason: "web_search",
  score: 0.7,
  confidence: "medium",
  createdAt: "2026-06-22T00:00:00Z",
};

describe("EvidenceDetailArtifactView", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("renders document-style evidence headings and metadata-only web notice", () => {
    act(() => {
      root.render(
        createElement(EvidenceDetailArtifactView, {
          payload: { sessionId: 10, evidence: [localEvidence, webEvidence] },
        }),
      );
    });

    expect(container.textContent).toContain("Evidence Detail");
    expect(container.textContent).toContain("[C1] Local Case");
    expect(container.textContent).toContain("[C2] Official Web");
    expect(container.textContent).toContain("Cases/Alpha.md");
    expect(container.textContent).toContain("source_unchanged");
    expect(container.textContent).toContain(
      "External webpage; page body and excerpt were not saved.",
    );
  });
});
