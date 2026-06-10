import { describe, expect, it } from "vitest";

import {
  availableMoveFolders,
  defaultScenesForKind,
  normalizeDocumentName,
  normalizeFolderPrefix,
  slugFromPath,
} from "@/components/file/vault-navigator-model";

describe("vault navigator model helpers", () => {
  it("normalizes corpus ids and default scenes", () => {
    expect(slugFromPath("法规/2026/")).toBe("法规_2026");
    expect(defaultScenesForKind("regulation")).toEqual(["knowledge_lookup"]);
    expect(defaultScenesForKind("exemplar")).toEqual([
      "exemplar_learning",
      "drafting_assist",
    ]);
    expect(defaultScenesForKind("general")).toEqual([]);
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
