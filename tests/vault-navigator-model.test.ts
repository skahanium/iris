import { describe, expect, it } from "vitest";

import {
  availableMoveFolders,
  canonicalCorpusKind,
  defaultScenesForKind,
  normalizeDocumentName,
  normalizeFolderPrefix,
  slugFromPath,
} from "@/components/file/vault-navigator-model";

describe("vault navigator model helpers", () => {
  it("normalizes corpus ids and default scenes", () => {
    expect(slugFromPath("法规/2026/")).toBe("法规_2026");
    expect(defaultScenesForKind("authority")).toEqual([
      "knowledge_lookup",
      "research_synthesis",
      "drafting_assist",
    ]);
    expect(defaultScenesForKind("exemplar")).toEqual([
      "exemplar_learning",
      "drafting_assist",
    ]);
    expect(defaultScenesForKind("reference")).toEqual([
      "knowledge_lookup",
      "research_synthesis",
    ]);
    expect(defaultScenesForKind("lookup")).toEqual([
      "knowledge_lookup",
      "research_synthesis",
    ]);
  });

  it("maps legacy corpus kinds to current roles", () => {
    expect(canonicalCorpusKind("regulation")).toBe("authority");
    expect(canonicalCorpusKind("general")).toBe("lookup");
    expect(canonicalCorpusKind("exemplar")).toBe("exemplar");
    expect(canonicalCorpusKind("unknown")).toBe("authority");
  });

  it("normalizes folder and document names", () => {
    expect(normalizeFolderPrefix("\\部门/制度")).toBe("部门/制度/");
    expect(normalizeDocumentName("会议纪要")).toBe("会议纪要.md");
    expect(normalizeDocumentName("README.MD")).toBe("README.MD");
  });

  it("excludes a moved folder and its descendants from move targets", () => {
    expect(
      availableMoveFolders(["a/", "a/b/", "z/"], {
        kind: "folder",
        path: "a/",
      }),
    ).toEqual(["", "z/"]);
  });
});
