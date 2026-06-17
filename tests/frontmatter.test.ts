import { describe, expect, it } from "vitest";

import {
  quoteYamlString,
  serializeNoteMarkdown,
  splitFrontmatter,
  titleFromFields,
} from "@/lib/frontmatter";

describe("frontmatter", () => {
  it("splits obsidian-style frontmatter", () => {
    const md = '---\ntitle: "Hello"\ntags: [a, b]\n---\n\nBody text.';
    const { yaml, fields, body } = splitFrontmatter(md);
    expect(yaml).toContain("title:");
    expect(titleFromFields(fields)).toBe("Hello");
    expect(fields.tags).toEqual(["a", "b"]);
    expect(body.trim()).toBe("Body text.");
  });

  it("quotes YAML strings safely", () => {
    expect(quoteYamlString('Say "hi"')).toBe('"Say \\"hi\\""');
  });

  it("serializes title and preserves other fields", () => {
    const existing = 'title: "Old"\ntags: [work]';
    const out = serializeNoteMarkdown(existing, "New Title", "Paragraph.");
    expect(out).toContain('title: "New Title"');
    expect(out).toContain("tags: [work]");
    expect(out).toContain("Paragraph.");
  });

  it("preserves unsupported complex YAML instead of rewriting it", () => {
    const existing = [
      'title: "Old"',
      "aliases:",
      "  - Alpha",
      "  - Beta",
      "nested:",
      "  owner:",
      "    name: Iris",
    ].join("\n");

    const out = serializeNoteMarkdown(existing, "New Title", "Paragraph.");

    expect(out).toContain('title: "New Title"');
    expect(out).toContain("aliases:\n  - Alpha\n  - Beta");
    expect(out).toContain("nested:\n  owner:\n    name: Iris");
  });
});
