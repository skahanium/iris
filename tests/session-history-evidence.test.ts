import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  evidenceRecordsToContextPackets,
  toChatLines,
} from "@/lib/ai/session-history";
import type { ContextPacket } from "@/types/ai";
import type { SessionMessageRecord } from "@/types/ipc";

const packet: ContextPacket = {
  id: "packet-1",
  source_type: "note",
  source_path: "Sources/Case.md",
  title: "Case Source",
  heading_path: null,
  source_span: null,
  content_hash: "hash-1",
  excerpt: "important evidence",
  retrieval_reason: "semantic",
  score: 0.9,
  trust_level: "user_note",
  citation_label: "S1",
  stale: false,
};

describe("session history evidence packets", () => {
  it("keeps loaded evidence packets on assistant chat lines", () => {
    const records: SessionMessageRecord[] = [
      {
        id: 1,
        session_id: 10,
        seq: 1,
        role: "assistant",
        content: "answer with [S1]",
        evidence_packets: [packet],
        created_at: "2026-06-22T00:00:00Z",
      },
    ];

    expect(toChatLines(records)).toEqual([
      {
        role: "assistant",
        content: "answer with [S1]",
        evidencePackets: [packet],
        seq: 1,
        created_at: "2026-06-22T00:00:00Z",
      },
    ]);
  });

  it("normalizes legacy sanitized message evidence packets before restoring", () => {
    const records = [
      {
        id: 2,
        session_id: 10,
        seq: 2,
        role: "assistant",
        content: "answer with [C1]",
        evidence_packets: [
          {
            id: "packet-legacy",
            source_type: "note",
            source_path: "Sources/Legacy.md",
            title: "Legacy Source",
            citation_label: "[C1]",
          },
        ],
        created_at: "2026-06-22T00:00:00Z",
      },
    ] as unknown as SessionMessageRecord[];

    expect(toChatLines(records)).toEqual([
      {
        role: "assistant",
        content: "answer with [C1]",
        evidencePackets: [
          expect.objectContaining({
            id: "packet-legacy",
            source_type: "note",
            source_path: "Sources/Legacy.md",
            title: "Legacy Source",
            citation_label: "[C1]",
            excerpt: "",
            score: 0,
            trust_level: "user_note",
            stale: false,
          }),
        ],
        seq: 2,
        created_at: "2026-06-22T00:00:00Z",
      },
    ]);
  });

  it("drops malformed non-array message evidence packets on restore", () => {
    const records = [
      {
        id: 3,
        session_id: 10,
        seq: 3,
        role: "assistant",
        content: "answer",
        evidence_packets: "not-a-packet-array",
        created_at: "2026-06-22T00:00:00Z",
      },
    ] as unknown as SessionMessageRecord[];

    expect(toChatLines(records)).toEqual([
      {
        role: "assistant",
        content: "answer",
        seq: 3,
        created_at: "2026-06-22T00:00:00Z",
      },
    ]);
  });

  it("converts session ledger records to drawer packets", () => {
    const packets = evidenceRecordsToContextPackets([
      {
        id: 1,
        sessionId: 10,
        citationIndex: 1,
        citationLabel: "[C1]",
        packetKey: "local:key",
        messageSeqFirst: 2,
        sourceType: "local",
        title: "Ledger Source",
        sourcePath: "Sources/Ledger.md",
        sourceSpanStart: 1,
        sourceSpanEnd: 8,
        headingPath: "Facts",
        contentHash: "hash-ledger",
        retrievalReason: "semantic",
        score: 0.8,
        confidence: "high",
        createdAt: "2026-06-22T00:00:00Z",
      },
    ]);

    expect(packets).toMatchObject([
      {
        id: "local:key",
        source_type: "note",
        source_path: "Sources/Ledger.md",
        citation_label: "[C1]",
        excerpt: "",
      },
    ]);
  });

  it("passes restored session ledger packets through the header into conversation state", () => {
    const header = readFileSync(
      "src/components/ai/AssistantPanelHeader.tsx",
      "utf8",
    );
    const panel = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );

    expect(header).toMatch(
      /onSelectSession:\s*\(\s*id: number,\s*messages: ChatLine\[\],\s*ledgerPackets\?: ChatLine\["evidencePackets"\],\s*\) => void/,
    );
    expect(panel).toContain("...args: Parameters<typeof handleLoadSession>");
    expect(panel).toContain("handleLoadSession(...args)");
  });
});
