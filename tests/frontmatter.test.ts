import { describe, expect, it } from "vitest";

import {
  quoteYamlString,
  serializeNoteMarkdown,
  splitFrontmatter,
} from "@/lib/frontmatter";

describe("frontmatter", () => {
  it("splits obsidian-style frontmatter", () => {
    const md = '---\ntitle: "Hello"\ntags: [a, b]\n---\n\nBody text.';
    const { yaml, fields, body } = splitFrontmatter(md);
    expect(yaml).toContain("title:");
    expect(fields.title).toBe("Hello");
    expect(fields.tags).toEqual(["a", "b"]);
    expect(body.trim()).toBe("Body text.");
  });

  it("quotes YAML strings safely", () => {
    expect(quoteYamlString('Say "hi"')).toBe('"Say \\"hi\\""');
  });

  it("removes the legacy system title and preserves other fields", () => {
    const existing = 'title: "Old"\ntags: [work]';
    const out = serializeNoteMarkdown(existing, "Paragraph.");
    expect(out).not.toContain("title:");
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

    const out = serializeNoteMarkdown(existing, "Paragraph.");

    expect(out).not.toContain("title:");
    expect(out).toContain("aliases:\n  - Alpha\n  - Beta");
    expect(out).toContain("nested:\n  owner:\n    name: Iris");
  });

  it("does not create frontmatter for a new blank note", () => {
    expect(serializeNoteMarkdown(null, "")).toBe("");
  });

  it("removes a multiline legacy title without touching neighbouring YAML", () => {
    const existing = [
      "# user comment",
      "title: |-",
      "  historical title",
      "  second line",
      "tags: [work]",
    ].join("\n");

    const out = serializeNoteMarkdown(existing, "Body.");

    expect(out).not.toContain("historical title");
    expect(out).toContain("# user comment");
    expect(out).toContain("tags: [work]");
  });
});
