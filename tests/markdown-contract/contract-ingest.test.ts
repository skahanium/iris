/**
 * contract-ingest.test.ts — TDD 红灯测试
 *
 * 直接测试 ingestMarkdown() 的行为规范。
 * 当前所有测试必须 FAIL（contract 尚未实现）。
 * 阶段 2.1 实现后，这些测试变为 GREEN。
 *
 * 覆盖 CONTRACT_PLAN.md § Source Ingest：
 * - 原始 Markdown 文本不可变保留
 * - 来源 profile 正确记录
 * - 流式状态正确传递
 * - 上下文信息正确捕获
 * - 解析后的 fragment 列表完整
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { ingestMarkdown } from "@/lib/markdown-contract/contract";
import type { MarkdownProfile } from "@/lib/markdown-contract/types";

const GOLD_ROOT = resolve(__dirname, "gold-corpus");
const BASIC_GFM = readFileSync(resolve(GOLD_ROOT, "basic-gfm.md"), "utf8");

// ── 基本摄取 ─────────────────────────────────────────────────

describe("ingest: basic ingestion", () => {
  it("[TDD-FAIL] raw markdown is preserved unchanged", () => {
    const source = "# Title\n\n**Bold** content with `code`.";
    const ingested = ingestMarkdown(source, {
      profile: "chat_assistant",
    });
    expect(ingested.raw).toBe(source);
  });

  it("[TDD-FAIL] source.profile reflects the profile option", () => {
    const ingested = ingestMarkdown("# Hello", {
      profile: "chat_assistant",
    });
    expect(ingested.source.profile).toBe("chat_assistant");
  });

  it("[TDD-FAIL] source.streaming defaults to false", () => {
    const ingested = ingestMarkdown("# Hello");
    expect(ingested.source.streaming).toBe(false);
  });

  it("[TDD-FAIL] source.streaming is true when explicitly set", () => {
    const ingested = ingestMarkdown("**partial", { streaming: true });
    expect(ingested.source.streaming).toBe(true);
  });

  it("[TDD-FAIL] source.context captures optional context string", () => {
    const ingested = ingestMarkdown("text", {
      context: "file://notes/test.md",
    });
    expect(ingested.source.context).toBe("file://notes/test.md");
  });

  it("[TDD-FAIL] fragments array is not empty for non-empty input", () => {
    const ingested = ingestMarkdown("# Title\n\nParagraph.");
    expect(ingested.fragments.length).toBeGreaterThan(0);
  });

  it("[TDD-FAIL] fragments array is empty for empty input", () => {
    const ingested = ingestMarkdown("");
    expect(ingested.fragments.length).toBe(0);
  });
});

// ── 大文档摄取 ────────────────────────────────────────────────

describe("ingest: large document", () => {
  it("[TDD-FAIL] full basic-gfm gold corpus is ingested with correct raw", () => {
    const ingested = ingestMarkdown(BASIC_GFM);
    expect(ingested.raw).toBe(BASIC_GFM);
    expect(ingested.fragments.length).toBeGreaterThan(10);
  });

  it("[TDD-FAIL] all profile types can be used for ingestion", () => {
    const profiles: MarkdownProfile[] = [
      "chat_assistant",
      "chat_user",
      "editor_ingest",
      "editor_export",
      "vault_preview",
    ];

    for (const p of profiles) {
      const ingested = ingestMarkdown("**test**", {
        profile: p,
      });
      expect(ingested.source.profile).toBe(p);
    }
  });
});

// ── 片段结构验证 ──────────────────────────────────────────────

describe("ingest: fragment structure", () => {
  it("[TDD-FAIL] each fragment has required fields", () => {
    const ingested = ingestMarkdown(
      "# Title\n\nParagraph **bold**.\n\n- list item",
    );

    for (const f of ingested.fragments) {
      expect(typeof f.raw).toBe("string");
      expect(f.raw.length).toBeGreaterThan(0);
      expect(typeof f.syntaxKind).toBe("string");
      expect(f.syntaxKind.length).toBeGreaterThan(0);
      expect(typeof f.capability).toBe("string");
      expect(typeof f.offset).toBe("number");
      expect(typeof f.endOffset).toBe("number");
      expect(f.endOffset).toBeGreaterThan(f.offset);
    }
  });

  it("[TDD-FAIL] fragments are ordered by offset (no overlap, no gaps)", () => {
    const source = "# H1\n\n**Bold**.\n\n- item 1\n- item 2";
    const ingested = ingestMarkdown(source);

    let expectedOffset = 0;
    for (const f of ingested.fragments) {
      expect(f.offset).toBe(expectedOffset);
      expectedOffset = f.endOffset;
    }
    expect(expectedOffset).toBe(source.length);
  });

  it("[TDD-FAIL] first fragment starts at offset 0", () => {
    const ingested = ingestMarkdown("# Title");
    expect(ingested.fragments[0]?.offset).toBe(0);
  });

  it("[TDD-FAIL] last fragment ends at source length", () => {
    const source = "# Title\n\nContent.";
    const ingested = ingestMarkdown(source);
    const last = ingested.fragments[ingested.fragments.length - 1];
    expect(last?.endOffset).toBe(source.length);
  });
});

// ── 幂等性 ────────────────────────────────────────────────────

describe("ingest: idempotency", () => {
  it("[TDD-FAIL] same input produces identical fragments across calls", () => {
    const source = "# Title\n\n**Bold** `code` - list";
    const r1 = ingestMarkdown(source);
    const r2 = ingestMarkdown(source);

    expect(r1.raw).toBe(r2.raw);
    expect(r1.fragments.length).toBe(r2.fragments.length);

    for (let i = 0; i < r1.fragments.length; i++) {
      expect(r1.fragments[i]!.raw).toBe(r2.fragments[i]!.raw);
      expect(r1.fragments[i]!.syntaxKind).toBe(r2.fragments[i]!.syntaxKind);
      expect(r1.fragments[i]!.capability).toBe(r2.fragments[i]!.capability);
      expect(r1.fragments[i]!.offset).toBe(r2.fragments[i]!.offset);
    }
  });
});
