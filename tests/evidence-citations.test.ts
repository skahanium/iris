import { describe, expect, it } from "vitest";

import {
  replaceAiCitationsForDocument,
  resolveCitationToEvidence,
  type CitationEvidenceRecord,
} from "@/lib/ai/evidence-citations";

const ledger: CitationEvidenceRecord[] = [
  {
    citationLabel: "[C1]",
    sourceType: "local",
    title: "Local Note",
    sourcePath: "Cases/Alpha.md",
  },
  {
    citationLabel: "[C2]",
    sourceType: "web",
    title: "Official Source",
    url: "https://example.com/report",
    domain: "example.com",
  },
  {
    citationLabel: "[C3]",
    sourceType: "web",
    title: "",
    url: "https://fallback.example/path",
    domain: "fallback.example",
  },
];

describe("AI citation replacement for documents", () => {
  it("resolves citations by label", () => {
    expect(resolveCitationToEvidence("[C1]", ledger)?.title).toBe("Local Note");
    expect(resolveCitationToEvidence("C1", ledger)?.title).toBe("Local Note");
    expect(resolveCitationToEvidence("[C99]", ledger)).toBeNull();
  });

  it("converts local and web citations to standard Iris document links", () => {
    const result = replaceAiCitationsForDocument("See [C1] and [C2].", ledger);
    expect(result.markdown).toBe(
      "See [[Cases/Alpha]] and [Official Source](https://example.com/report).",
    );
    expect(result.missing).toEqual([]);
  });

  it("converts adjacent citations one by one", () => {
    const result = replaceAiCitationsForDocument("Refs [C1][C2]", ledger);
    expect(result.markdown).toBe(
      "Refs [[Cases/Alpha]][Official Source](https://example.com/report)",
    );
  });

  it("uses domain fallback for untitled web evidence", () => {
    const result = replaceAiCitationsForDocument("See [C3].", ledger);
    expect(result.markdown).toBe(
      "See [fallback.example](https://fallback.example/path).",
    );
  });

  it("keeps missing citations and reports them", () => {
    const result = replaceAiCitationsForDocument("Missing [C99].", ledger);
    expect(result.markdown).toBe("Missing [C99].");
    expect(result.missing).toEqual(["[C99]"]);
  });

  it("skips fenced code blocks and inline code", () => {
    const markdown = "```md\n[C1]\n```\nUse `[C2]` then [C1].";
    const result = replaceAiCitationsForDocument(markdown, ledger);
    expect(result.markdown).toBe(
      "```md\n[C1]\n```\nUse `[C2]` then [[Cases/Alpha]].",
    );
  });

  it("skips existing markdown links and wiki-links", () => {
    const markdown =
      "Already [[Page [C1]]] and [label [C2]](https://x.test) then [C1].";
    const result = replaceAiCitationsForDocument(markdown, ledger);
    expect(result.markdown).toBe(
      "Already [[Page [C1]]] and [label [C2]](https://x.test) then [[Cases/Alpha]].",
    );
  });
});
