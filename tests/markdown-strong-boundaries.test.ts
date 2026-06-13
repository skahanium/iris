import { describe, expect, it } from "vitest";
import { markdownToHtml } from "@/lib/markdown";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { editorHtmlHasVisibleFailedBold } from "@/lib/editor-html-cache";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

function visibleFailedBold(html: string): boolean {
  return editorHtmlHasVisibleFailedBold(`<div>${html}</div>`);
}

describe("commonmark strong boundary failures", () => {
  it("colon inside bold tight to text", () => {
    const md = "1. **CUDA Graph 显存调优：**优化 CUDA Graph 捕获范围。";
    const html = markdownToHtml(md);
    expect(html).toContain("<strong>");
    expect(html).not.toContain("**CUDA");
  });

  it("period inside bold tight to text", () => {
    const md =
      "前缀树节点虽然逻辑上仍代表一段完整 token 序列, 但它对应的 SWA KV **可能只剩最后一部分, 甚至已经完全不存在。**如果前缀树仍按规则给出复用长度。";
    const html = markdownToHtml(md);
    expect(visibleFailedBold(html)).toBe(false);
    expect(html).toContain("<strong>可能只剩最后一部分");
  });

  it("multiline bold ending with period before close", () => {
    const md = `前缀树节点虽然逻辑上仍代表一段完整 token 序列, 但它对应的 SWA KV **可能只剩最后
部分, 甚至已经完全不存在。**如果前缀树仍按规则给出复用长度。`;
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    const editor = createProductionEditorFromIngestedBody(md);
    try {
      expect(visibleFailedBold(tipTapHtml)).toBe(false);
      expect(editor.view.dom.textContent).not.toMatch(/\*\*可能只剩/);
    } finally {
      editor.destroy();
    }
  });

  it("full-document ingest repairs bold split across soft line break", () => {
    const md = `前缀树节点 SWA KV **可能只剩最后
部分, 甚至已经完全不存在。**如果前缀树仍按规则给出复用长度。`;
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(visibleFailedBold(tipTapHtml)).toBe(false);
  });
});

describe("mimo-like prose patterns", () => {
  it("renders percent bold followed by CJK text", () => {
    const md =
      "服务端 KV Cache 命中率平均可达 **93%**；对于高强度用户，该指标更可攀升至 **95%** 以上乃至更高。";
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(visibleFailedBold(tipTapHtml)).toBe(false);
    expect(tipTapHtml).toContain("<strong>95%</strong>");
  });

  it("does not flag literal stars inside fenced code", () => {
    const md = `正文段落。

\`\`\`md
**示例：**不会渲染
\`\`\``;
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(tipTapHtml).toContain("**示例：**");
    expect(visibleFailedBold(tipTapHtml)).toBe(false);
  });

  it("repairs at parse time without html_comment fragment split", () => {
    const md = "前缀 **标题：**正文继续";
    const frags = classifyMarkdownCapabilities(md);
    expect(frags.some((f) => f.syntaxKind === "html_comment")).toBe(false);
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(visibleFailedBold(tipTapHtml)).toBe(false);
    expect(tipTapHtml).toContain("<strong>标题：</strong>");
  });
});
