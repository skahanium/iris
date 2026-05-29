import { describe, expect, it } from "vitest";
import { aiMarked } from "@/lib/ai/marked-ai";

function removeAttrQuirks(html: string): string {
  return html.replace(/=""/g, "");
}

describe("aiMarked code blocks", () => {
  it("renders fenced code block with syntax highlighting", () => {
    const html = aiMarked.parse("```ts\nconst x: number = 1;\n```") as string;
    expect(html).toContain("<pre>");
    expect(html).toContain("hljs");
    expect(html).toContain("language-ts");
    expect(html).toContain("const");
  });

  it("falls back gracefully for unknown language", () => {
    // Unknown language with short text → highlightAuto returns empty → falls back to plain text
    const html = aiMarked.parse("```zzzlang\nfoo bar\n```") as string;
    expect(html).toContain("<pre>");
    expect(html).toContain("foo bar");
    expect(html).toContain("language-zzzlang");
  });

  it("renders code block with no language", () => {
    const html = aiMarked.parse("```\nplain text\n```") as string;
    expect(html).toContain("<pre>");
    expect(html).toContain("plain text");
  });
});

describe("aiMarked tables", () => {
  it("wraps table in ai-table-wrap div", () => {
    const html = aiMarked.parse("| A | B |\n| --- | --- |\n| 1 | 2 |") as string;
    expect(html).toContain("ai-table-wrap");
    expect(html).toContain("<table>");
    expect(html).toContain("<thead>");
    expect(html).toContain("<tbody>");
  });

  it("renders table headers and cells", () => {
    const html = aiMarked.parse("| Name | Value |\n| --- | --- |\n| foo | 42 |") as string;
    expect(html).toContain("Name");
    expect(html).toContain("Value");
    expect(html).toContain("foo");
    expect(html).toContain("42");
  });
});

describe("aiMarked task lists", () => {
  it("renders checked task list item with checkbox", () => {
    const html = aiMarked.parse("- [x] Completed task") as string;
    expect(html).toContain('type="checkbox"');
    expect(html).toContain("checked");
    expect(html).toContain("task-list-item");
    expect(html).toContain("Completed task");
  });

  it("renders unchecked task list item", () => {
    const html = aiMarked.parse("- [ ] Pending task") as string;
    expect(html).toContain('type="checkbox"');
    expect(html).not.toContain("checked");
    expect(html).toContain("Pending task");
  });
});

describe("aiMarked links", () => {
  it("adds target=_blank and rel=noopener to external links", () => {
    const html = aiMarked.parse("[Example](https://example.com)") as string;
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain("Example");
  });

  it("does not add target=_blank to citation links", () => {
    const html = aiMarked.parse(
      "[citation:1](#iris-cite-citation%3A1)",
    ) as string;
    const cleaned = removeAttrQuirks(html);
    expect(cleaned).not.toContain('target="_blank"');
    expect(cleaned).toContain("#iris-cite-");
  });
});

describe("aiMarked inline formatting", () => {
  it("renders bold text", () => {
    const html = aiMarked.parse("Hello **world**") as string;
    expect(html).toContain("<strong>world</strong>");
  });

  it("renders italic text", () => {
    const html = aiMarked.parse("Hello *world*") as string;
    expect(html).toContain("<em>world</em>");
  });

  it("renders inline code", () => {
    const html = aiMarked.parse("Use `const` here") as string;
    expect(html).toContain("<code>const</code>");
  });

  it("renders strikethrough", () => {
    const html = aiMarked.parse("~~removed~~") as string;
    expect(html).toContain("<del>removed</del>");
  });
});

describe("aiMarked block elements", () => {
  it("renders blockquote", () => {
    const html = aiMarked.parse("> quoted text") as string;
    expect(html).toContain("<blockquote>");
    expect(html).toContain("quoted text");
  });

  it("renders headings", () => {
    const html = aiMarked.parse("# H1\n## H2\n### H3") as string;
    expect(html).toContain("<h1>H1</h1>");
    expect(html).toContain("<h2>H2</h2>");
    expect(html).toContain("<h3>H3</h3>");
  });

  it("renders unordered list", () => {
    const html = aiMarked.parse("- alpha\n- beta") as string;
    expect(html).toContain("<ul>");
    expect(html).toContain("alpha");
    expect(html).toContain("beta");
  });

  it("renders ordered list", () => {
    const html = aiMarked.parse("1. First\n2. Second") as string;
    expect(html).toContain("<ol>");
    expect(html).toContain("First");
    expect(html).toContain("Second");
  });

  it("renders horizontal rule", () => {
    const html = aiMarked.parse("before\n\n---\n\nafter") as string;
    expect(html).toContain("<hr");
  });
});
