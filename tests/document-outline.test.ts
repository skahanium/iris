import { Schema } from "@tiptap/pm/model";
import { describe, expect, it } from "vitest";

import { activeOutlineIndex, outlineFromDoc } from "@/lib/document-outline";

const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { group: "block", content: "text*" },
    heading: {
      group: "block",
      content: "text*",
      attrs: { level: { default: 1 } },
    },
    text: { group: "inline" },
  },
});

function docWithHeadings() {
  return schema.node("doc", null, [
    schema.node("heading", { level: 1 }, [schema.text("一级")]),
    schema.node("paragraph", null, [schema.text("正文")]),
    schema.node("heading", { level: 2 }, [schema.text("二级")]),
    schema.node("heading", { level: 3 }, [schema.text("三级")]),
  ]);
}

describe("document-outline", () => {
  it("preserves internal heading spaces while trimming boundary spaces", () => {
    const doc = schema.node("doc", null, [
      schema.node("heading", { level: 1 }, [
        schema.text("  第一章    总 则  "),
      ]),
      schema.node("heading", { level: 2 }, [schema.text("    ")]),
    ]);

    const items = outlineFromDoc(doc);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      level: 1,
      text: "第一章    总 则",
    });
  });

  it("extracts section headings with positions", () => {
    const doc = docWithHeadings();
    const items = outlineFromDoc(doc);
    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({ level: 1, text: "一级" });
    expect(items[1]).toMatchObject({ level: 2, text: "二级" });
    expect(items[2]).toMatchObject({ level: 3, text: "三级" });
    expect(items[0]!.pos).toBeLessThan(items[1]!.pos);
  });

  it("resolves active heading from cursor head", () => {
    const doc = docWithHeadings();
    const items = outlineFromDoc(doc);
    const h2Pos = items[1]!.pos;
    const h3Pos = items[2]!.pos;
    expect(activeOutlineIndex(items, h2Pos)).toBe(1);
    expect(activeOutlineIndex(items, h2Pos + 1)).toBe(1);
    expect(activeOutlineIndex(items, h3Pos - 1)).toBe(1);
    expect(activeOutlineIndex(items, items[0]!.pos)).toBe(0);
  });
});
