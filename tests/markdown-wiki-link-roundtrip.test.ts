import { describe, expect, it } from "vitest";

import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("wiki-link and external link round-trip", () => {
  it("preserves wiki-links and markdown links together", () => {
    const body =
      "See [docs](https://example.com) and [[Target Note]] plus plain [[Not Linked]].";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toContain("[docs](https://example.com)");
      expect(md).toContain("[[Target Note]]");
      expect(md).toContain("[[Not Linked]]");
    } finally {
      editor.destroy();
    }
  });

  it("round-trips external link mark from ingest", () => {
    const body = "Visit [Iris](https://iris.example) today.";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("[Iris](https://iris.example)");
    } finally {
      editor.destroy();
    }
  });
});
