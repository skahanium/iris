import { describe, expect, it } from "vitest";

type AssistantRouteInput = {
  message: string;
  hasSelection: boolean;
  notePath: string | null;
  explicitScope: boolean;
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
