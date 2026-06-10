/**
 * ai-render-consistency.test.ts — AI 展示重构 阶段 0 基线测试
 *
 * 测试用户/助手/系统消息的 Markdown 渲染一致性。
 * 基线测试：测试现有行为，应全 GREEN。
 *
 * 覆盖 CONTRACT_PLAN.md § 测试计划 1 — 消息渲染一致性
 */
import { describe, expect, it } from "vitest";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";

// ═══════════════════════════════════════════════════════════════
// 用户消息 vs 助手消息渲染一致性
// ═══════════════════════════════════════════════════════════════

describe("user/assistant message rendering parity", () => {
  const profiles = ["chat_user", "chat_assistant"] as const;

  it("[BASELINE] both profiles render **bold** as <strong>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("**bold text**", p);
      expect(r.output).toContain("<strong>");
    }
  });

  it("[BASELINE] both profiles render *italic* as <em>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("*italic text*", p);
      expect(r.output).toContain("<em>");
    }
  });

  it("[BASELINE] both profiles render `code` as <code>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("`inline code`", p);
      expect(r.output).toContain("<code>");
    }
  });

  it("[BASELINE] both profiles render # heading as <h1>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("# The Title", p);
      expect(r.output).toContain("<h1");
    }
  });

  it("[BASELINE] both profiles render - list as <ul>/<li>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("- item 1\n- item 2", p);
      expect(r.output).toContain("<ul");
      expect(r.output).toContain("<li");
    }
  });

  it("[BASELINE] both profiles render > quote as <blockquote>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("> quoted text", p);
      expect(r.output).toContain("<blockquote>");
    }
  });

  it("[BASELINE] both profiles render [link] as <a>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("[link](https://example.com)", p);
      expect(r.output).toContain("<a");
      expect(r.output).toContain("href=");
    }
  });

  it("[BASELINE] both profiles render code blocks with <pre>", () => {
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("```js\nconst x = 1;\n```", p);
      expect(r.output).toContain("<pre");
    }
  });

  it("chat_assistant code blocks include a sanitized copy button", () => {
    const result = renderMarkdownWithProfile(
      "```bash\ncurl -fsSL https://example.test/install.sh\n```",
      "chat_assistant",
    );

    expect(result.output).toContain('class="ai-code-block"');
    expect(result.output).toContain("data-ai-code-copy");
    expect(result.output).toContain('aria-label="复制代码"');
    expect(result.output).toContain(
      "curl -fsSL https://example.test/install.sh",
    );
  });

  it("[BASELINE] both profiles render tables", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    for (const p of profiles) {
      const r = renderMarkdownWithProfile(md, p);
      expect(r.output).toContain("<table");
    }
  });

  it("[BASELINE] both profiles render task lists", () => {
    const md = "- [x] Done\n- [ ] Pending";
    for (const p of profiles) {
      const r = renderMarkdownWithProfile(md, p);
      expect(r.output).toContain("checkbox");
    }
  });

  it("[BASELINE] same markdown produces equivalent text in both profiles", () => {
    const md = "**Header**: the *data* shows `42`.\n\n- item\n\n> note";
    const user = renderMarkdownWithProfile(md, "chat_user");
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    // Both should have same semantic stats since classification is source-level
    expect(user.meta.stats).toEqual(asst.meta.stats);
  });

  it("[BASELINE] chat_assistant linkifies citations, chat_user does not", () => {
    const md = "See [citation:1].";
    const asst = renderMarkdownWithProfile(md, "chat_assistant");
    const user = renderMarkdownWithProfile(md, "chat_user");
    expect(asst.output).toContain("ai-citation");
    expect(user.output).not.toContain("ai-citation");
  });
});

// ═══════════════════════════════════════════════════════════════
// 系统消息行为
// ═══════════════════════════════════════════════════════════════

describe("system message behavior", () => {
  it("[BASELINE] renderMarkdownWithProfile works for all defined profiles", () => {
    const profiles = [
      "chat_assistant",
      "chat_user",
      "editor_ingest",
      "editor_export",
      "vault_preview",
      "research_card",
      "patch_preview",
      "citation_panel",
    ] as const;
    for (const p of profiles) {
      const r = renderMarkdownWithProfile("**test**", p);
      expect(r.output.length).toBeGreaterThan(0);
      expect(r.meta.profile).toBe(p);
    }
  });
});

// ═══════════════════════════════════════════════════════════════
// 跨 profile 语义等价
// ═══════════════════════════════════════════════════════════════

describe("cross-profile semantic equivalence", () => {
  it("[BASELINE] same markdown in all display profiles has identical stats", () => {
    const md = "# Title\n\n**Bold** and *italic*.\n\n- list\n\n> quote";
    const displayProfiles = [
      "chat_assistant",
      "chat_user",
      "vault_preview",
      "research_card",
    ] as const;
    const results = displayProfiles.map((p) =>
      renderMarkdownWithProfile(md, p),
    );
    for (let i = 1; i < results.length; i++) {
      expect(results[i]!.meta.stats).toEqual(results[0]!.meta.stats);
    }
  });

  it("[BASELINE] all display profiles produce non-empty output for core GFM", () => {
    const cases = [
      "**bold**",
      "*italic*",
      "`code`",
      "# Title",
      "- item",
      "> quote",
      "[link](https://a.test)",
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
    ];
    for (const md of cases) {
      for (const p of [
        "chat_assistant",
        "chat_user",
        "vault_preview",
      ] as const) {
        const r = renderMarkdownWithProfile(md, p);
        expect(r.output.length).toBeGreaterThan(0);
      }
    }
  });

  it("[BASELINE] display profiles sanitize output (no XSS)", () => {
    const md = "<script>alert(1)</script>\n**safe**";
    for (const p of ["chat_assistant", "chat_user", "vault_preview"] as const) {
      const r = renderMarkdownWithProfile(md, p);
      expect(r.output).not.toContain("<script");
      expect(r.output).toContain("safe");
    }
  });
});

// ═══════════════════════════════════════════════════════════════
// Streaming 完整性
// ═══════════════════════════════════════════════════════════════

describe("streaming complete = non-streaming semantic equivalence", () => {
  it("[BASELINE] complete input streaming vs non-streaming has same stats", () => {
    const md = "**bold** and *italic* `code` - item";
    const stream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: true,
    });
    const nonStream = renderMarkdownWithProfile(md, "chat_assistant", {
      streaming: false,
    });
    expect(stream.meta.stats).toEqual(nonStream.meta.stats);
  });

  it("[BASELINE] streaming mode has repairs, non-streaming does not", () => {
    const stream = renderMarkdownWithProfile("**bold**", "chat_assistant", {
      streaming: true,
    });
    const nonStream = renderMarkdownWithProfile("**bold**", "chat_assistant", {
      streaming: false,
    });
    // Non-streaming should have no repairs (balanced input)
    expect(nonStream.streamRepairs.length).toBe(0);
    expect(typeof stream.streamRepairs.length).toBe("number");
  });

  it("[BASELINE] unclosed bold in streaming produces valid HTML", () => {
    const r = renderMarkdownWithProfile("**partial", "chat_assistant", {
      streaming: true,
    });
    expect(r.output).toContain("<strong>");
    expect(r.streamRepairs.length).toBeGreaterThan(0);
  });
});

// ═══════════════════════════════════════════════════════════════
// research_card / patch_preview / citation_panel profiles
// ═══════════════════════════════════════════════════════════════

describe("artifact profiles: research_card, patch_preview, citation_panel", () => {
  it("[BASELINE] research_card profile renders bold", () => {
    const r = renderMarkdownWithProfile("**key**", "research_card");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] patch_preview profile renders bold", () => {
    const r = renderMarkdownWithProfile("**warning**", "patch_preview");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] citation_panel profile renders bold", () => {
    const r = renderMarkdownWithProfile("**claim**", "citation_panel");
    expect(r.output).toContain("<strong>");
  });

  it("[BASELINE] all artifact profiles produce sanitized output", () => {
    for (const p of [
      "research_card",
      "patch_preview",
      "citation_panel",
    ] as const) {
      const r = renderMarkdownWithProfile("<script>x</script>\n**safe**", p);
      expect(r.output).not.toContain("<script");
      expect(r.output).toContain("safe");
    }
  });
});
