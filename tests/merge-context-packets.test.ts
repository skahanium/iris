import { describe, expect, it } from "vitest";

import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import type { ContextPacket } from "@/types/ai";

const packet = (id: string): ContextPacket =>
  ({
    id,
    source_type: "note",
    source_path: "/a.md",
    title: "A",
    heading_path: null,
    source_span: null,
    content_hash: "h",
    excerpt: "…",
    retrieval_reason: "test",
    score: 1,
    trust_level: "user_note",
    citation_label: "[1]",
    stale: false,
    web: null,
  }) as ContextPacket;

describe("mergeContextPackets", () => {
  it("dedupes by id and preserves order", () => {
    const merged = mergeContextPackets(
      [packet("a"), packet("b")],
      [packet("b"), packet("c")],
    );
    expect(merged.map((p) => p.id)).toEqual(["a", "b", "c"]);
  });
});
