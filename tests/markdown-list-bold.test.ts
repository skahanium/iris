import { describe, expect, it } from "vitest";

import {
  parseMarkdownToHtml,
  renderAiMarkdownToHtml,
} from "@/lib/markdown-render";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { repairTightStrongPunctuationBoundaries } from "@/lib/markdown";
import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

describe("markdown list inline bold", () => {
  it("renders **bold** inside unordered list items", () => {
    const md = "- **解散议会争议**：马克龙在2025年解散国民议会。";
    const html = parseMarkdownToHtml(md);
    expect(html).toContain("<strong>解散议会争议</strong>");
    expect(html).not.toContain("**解散议会争议**");
  });

  it("chat_assistant profile renders bold in lists", () => {
    const md = `**马克龙近期动态**

**1. 政治与国内局势**

- **解散议会争议**：马克龙在2025年解散国民议会。
- **拟用公投破僵局**：马克龙表示2025年可能举行公投。`;
    const { output } = renderMarkdownWithProfile(md, "chat_assistant");
    expect(output).toContain("<strong>解散议会争议</strong>");
    expect(output).toContain("<strong>拟用公投破僵局</strong>");
    expect(output).not.toContain("**解散议会争议**");
  });

  it("renderAiMarkdownToHtml matches parse for list bold", () => {
    const md = "- **Key**: value";
    const html = renderAiMarkdownToHtml(md);
    expect(html).toContain("<strong>Key</strong>");
  });

  it("renders tight bold labels that include a colon", () => {
    const md = "- **CUDA Graph 显存调优：**优化 CUDA Graph 捕获范围。";
    const html = parseMarkdownToHtml(md);
    expect(html).toContain("<strong>CUDA Graph 显存调优：</strong>");
    expect(html).not.toContain("**CUDA Graph 显存调优：**");
  });

  it("renders tight bold labels that end with a Chinese colon", () => {
    const md = "- **DP-Attention 同步：**多 DP 段的计算拖慢。";
    const html = parseMarkdownToHtml(md);
    expect(html).toContain("<strong>DP-Attention 同步：</strong>");
    expect(html).not.toContain("**DP-Attention 同步：**");
  });

  it("leaves tight bold syntax unchanged inside inline code", () => {
    const md = "`**DP-Attention 同步：**多 DP 段的计算拖慢。`";

    expect(repairTightStrongPunctuationBoundaries(md)).toBe(md);
  });

  it("leaves tight bold syntax unchanged inside fenced code", () => {
    const md = "```md\n**DP-Attention 同步：**多 DP 段的计算拖慢。\n```";

    expect(repairTightStrongPunctuationBoundaries(md)).toBe(md);
  });

  it("ingests ordered list bold ending with Chinese colon as strong marks", () => {
    const md = "1. **CUDA Graph 显存调优：**优化 CUDA Graph 捕获范围。";
    const editor = createProductionEditorFromIngestedBody(md);

    try {
      const strong = editor.view.dom.querySelector("strong");
      expect(strong?.textContent).toBe("CUDA Graph 显存调优：");
      expect(editor.view.dom.textContent).not.toContain(
        "**CUDA Graph 显存调优：**",
      );
    } finally {
      editor.destroy();
    }
  });

  it("ingests ordered list bold ending with Chinese colon before text as strong marks", () => {
    const md = "1. **DP-Attention 同步：**多 DP 段的计算拖慢。";
    const editor = createProductionEditorFromIngestedBody(md);

    try {
      const strong = editor.view.dom.querySelector("strong");
      expect(strong?.textContent).toBe("DP-Attention 同步：");
      expect(editor.view.dom.textContent).not.toContain(
        "**DP-Attention 同步：**",
      );
    } finally {
      editor.destroy();
    }
  });

  it("ingests tight colon-bold labels with curly quotes and no space after closing **", () => {
    const md =
      "1. **匹配规则升级为\u201c窗口安全长度\u201d：**在 SWA 模式下";
    const editor = createProductionEditorFromIngestedBody(md);

    try {
      const strong = editor.view.dom.querySelector("strong");
      expect(strong?.textContent).toBe("匹配规则升级为“窗口安全长度”：");
      expect(editor.view.dom.textContent).not.toContain("**匹配规则");
    } finally {
      editor.destroy();
    }
  });
});
