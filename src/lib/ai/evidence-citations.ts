import type { ContextPacket } from "@/types/ai";

export interface CitationEvidenceRecord {
  citationLabel: string;
  sourceType: "local" | "web" | string;
  title?: string | null;
  sourcePath?: string | null;
  url?: string | null;
  domain?: string | null;
}

export interface CitationReplacementResult {
  markdown: string;
  missing: string[];
}

export function citationRecordsFromContextPackets(
  packets: ContextPacket[] | undefined,
): CitationEvidenceRecord[] {
  return (packets ?? []).map((packet) => ({
    citationLabel: packet.citation_label,
    sourceType: packet.source_type === "web" ? "web" : "local",
    title: packet.title,
    sourcePath: packet.source_path,
    url:
      packet.web?.url ??
      (packet.source_type === "web" ? packet.source_path : null),
    domain: packet.web?.domain ?? null,
  }));
}

const CITATION_RE = /^\[C\d+\]/;

export function resolveCitationToEvidence(
  ref: string,
  ledger: CitationEvidenceRecord[],
): CitationEvidenceRecord | null {
  const label = normalizeCitationRef(ref);
  return ledger.find((item) => item.citationLabel === label) ?? null;
}

export function replaceAiCitationsForDocument(
  markdown: string,
  ledger: CitationEvidenceRecord[],
): CitationReplacementResult {
  const missing = new Set<string>();
  const lines = markdown.split(/(\n)/);
  let inFence = false;
  const converted = lines.map((part) => {
    if (part === "\n") return part;
    if (isFenceLine(part)) {
      inFence = !inFence;
      return part;
    }
    if (inFence) return part;
    return replaceCitationsInLine(part, ledger, missing);
  });

  return {
    markdown: converted.join(""),
    missing: Array.from(missing),
  };
}

function normalizeCitationRef(ref: string): string {
  const trimmed = ref.trim();
  if (/^\[C\d+\]$/.test(trimmed)) return trimmed;
  if (/^C\d+$/.test(trimmed)) return `[${trimmed}]`;
  return trimmed;
}

function isFenceLine(line: string): boolean {
  const trimmed = line.trimStart();
  return trimmed.startsWith("```") || trimmed.startsWith("~~~");
}

function replaceCitationsInLine(
  line: string,
  ledger: CitationEvidenceRecord[],
  missing: Set<string>,
): string {
  let out = "";
  let index = 0;
  while (index < line.length) {
    if (line[index] === "`") {
      const end = line.indexOf("`", index + 1);
      if (end === -1) {
        out += line.slice(index);
        break;
      }
      out += line.slice(index, end + 1);
      index = end + 1;
      continue;
    }

    if (line.startsWith("[[", index)) {
      const end = line.indexOf("]]", index + 2);
      if (end === -1) {
        out += line.slice(index);
        break;
      }
      out += line.slice(index, end + 2);
      index = end + 2;
      continue;
    }

    const markdownLinkEnd = markdownLinkEndAt(line, index);
    if (markdownLinkEnd != null) {
      out += line.slice(index, markdownLinkEnd);
      index = markdownLinkEnd;
      continue;
    }

    const citation = line.slice(index).match(CITATION_RE)?.[0];
    if (citation) {
      const evidence = resolveCitationToEvidence(citation, ledger);
      if (evidence) {
        out += citationToDocumentLink(evidence);
      } else {
        out += citation;
        missing.add(citation);
      }
      index += citation.length;
      continue;
    }

    out += line[index];
    index += 1;
  }
  return out;
}

function markdownLinkEndAt(line: string, start: number): number | null {
  if (line[start] !== "[" || line.startsWith("[[", start)) return null;
  let depth = 0;
  for (let index = start; index < line.length; index += 1) {
    const char = line[index];
    if (char === "[") depth += 1;
    if (char === "]") {
      depth -= 1;
      if (depth === 0) {
        if (line[index + 1] !== "(") return null;
        const close = line.indexOf(")", index + 2);
        return close === -1 ? null : close + 1;
      }
    }
  }
  return null;
}

function citationToDocumentLink(evidence: CitationEvidenceRecord): string {
  if (evidence.sourceType === "web") {
    const url = evidence.url?.trim();
    if (!url) return evidence.citationLabel;
    const title = evidence.title?.trim() || evidence.domain?.trim() || url;
    return `[${title}](${url})`;
  }

  const path = evidence.sourcePath?.trim();
  if (!path) return evidence.citationLabel;
  const normalized = path.replace(/\\/g, "/").replace(/\.md$/i, "");
  return `[[${normalized}]]`;
}
