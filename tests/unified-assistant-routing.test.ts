import { describe, expect, it } from "vitest";

type AssistantRouteInput = {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
  uiAction?: string;
  hasImage?: boolean;
  skillMention?: boolean;
};

async function loadRouter() {
  try {
    const spec = "@/lib/assistant-routing";
    return await import(/* @vite-ignore */ spec);
  } catch {
    return null;
  }
}

async function route(input: Partial<AssistantRouteInput>) {
  const mod = await loadRouter();

  expect(mod?.resolveAssistantIntent).toBeTypeOf("function");
  return mod!.resolveAssistantIntent({
    message: "",
    hasSelection: false,
    notePath: null,
    explicitScope: false,
    ...input,
  });
}

async function detect(input: Partial<AssistantRouteInput>) {
  const mod = await loadRouter();

  expect(mod?.detectAgentIntent).toBeTypeOf("function");
  return mod!.detectAgentIntent({
    message: "",
    hasSelection: false,
    notePath: null,
    explicitScope: false,
    ...input,
  });
}

describe("resolveAssistantIntent", () => {
  it("routes selection rewrite requests to writing", async () => {
    await expect(
      route({
        message: "帮我改写这段，让它更精炼",
        hasSelection: true,
        notePath: "notes/demo.md",
      }),
    ).resolves.toBe("writing");
  });

  it("routes citation checks to citation", async () => {
    await expect(
      route({
        message: "检查这一段的引用是否充分",
        hasSelection: true,
        notePath: "notes/demo.md",
      }),
    ).resolves.toBe("citation");
  });

  it("routes organize requests to organize", async () => {
    await expect(
      route({ message: "帮我整理一下整个资料库的标签和标题" }),
    ).resolves.toBe("organize");
  });

  it("routes research questions to research", async () => {
    await expect(
      route({
        message: "研究一下 sqlite-vec 和 FTS5 在本地知识库中的取舍",
        explicitScope: true,
      }),
    ).resolves.toBe("research");
  });

  it("falls back to knowledge lookup for ordinary search questions", async () => {
    await expect(
      route({
        message: "帮我查一下当前库里关于向量检索的内容",
      }),
    ).resolves.toBe("knowledge");
  });

  it("falls back to chat when no stronger intent exists", async () => {
    await expect(route({ message: "我们来聊聊这篇笔记的思路" })).resolves.toBe(
      "chat",
    );
  });

  it("labels chat intent as 对话 not legacy 自由对话", async () => {
    const mod = await loadRouter();
    expect(mod?.assistantIntentLabel("chat")).toBe("对话");
  });

  it("routes chapter writing when the document has headings", async () => {
    await expect(
      route({
        message: "请改写本章的结构，让论证更紧凑",
        notePath: "notes/demo.md",
      }),
    ).resolves.toBe("chapter");
  });

  it("routes document checks for full-note audits", async () => {
    await expect(
      route({
        message: "做一次全文大纲检查",
        notePath: "notes/demo.md",
      }),
    ).resolves.toBe("document");
  });
});

describe("detectAgentIntent", () => {
  it("explains UI action priority for selection rewrite", async () => {
    const result = await detect({
      message: "处理一下",
      hasSelection: true,
      notePath: "notes/demo.md",
      uiAction: "rewrite",
    });

    expect(result.detectedIntent).toBe("rewrite_selection");
    expect(result.confidence).toBeGreaterThanOrEqual(0.9);
    expect(result.sourceHints).toContain("ui_action:rewrite");
    expect(result.reason).toContain("UI action");
  });

  it("detects note lookup as ask_notes with explanatory alternatives", async () => {
    const result = await detect({
      message: "帮我查一下当前库里关于向量检索的内容",
      notePath: "notes/demo.md",
    });

    expect(result.detectedIntent).toBe("ask_notes");
    expect(result.alternatives).toContain("chat");
    expect(result.fallbackBehavior).toContain("chat");
  });

  it("detects the Phase2 agent intents from natural input", async () => {
    await expect(
      detect({ message: "研究一下 sqlite-vec 和 FTS5 的取舍" }),
    ).resolves.toMatchObject({ detectedIntent: "research" });
    await expect(
      detect({ message: "帮我整理资料库的标签和标题" }),
    ).resolves.toMatchObject({ detectedIntent: "organize" });
    await expect(
      detect({
        message: "检查这一段的引用是否充分",
        hasSelection: true,
        notePath: "notes/demo.md",
      }),
    ).resolves.toMatchObject({ detectedIntent: "citation_check" });
    await expect(
      detect({ message: "这张图里有哪些信息？", hasImage: true }),
    ).resolves.toMatchObject({ detectedIntent: "vision_chat" });
    await expect(
      detect({ message: "安装这个 skill", skillMention: true }),
    ).resolves.toMatchObject({ detectedIntent: "skill_management" });
  });

  it("uses low-confidence chat fallback without exposing a scene selector", async () => {
    const result = await detect({ message: "随便聊聊" });

    expect(result.detectedIntent).toBe("chat");
    expect(result.confidence).toBeLessThan(0.7);
    expect(result.fallbackBehavior).toContain("suggest");
    expect(result.reason).not.toContain("scene selector");
  });
});
