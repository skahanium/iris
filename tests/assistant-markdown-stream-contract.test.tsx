import { existsSync, readFileSync } from "node:fs";

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageList } from "@/components/ai/AiMessageList";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 112,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        key: `row-${index}`,
        start: index * 112,
      })),
    measureElement: vi.fn(),
  }),
}));

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant markdown-first message stream", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("does not render research results as a custom message card", () => {
    expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
      "ResearchResultMessage",
    );
    expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
      'kind?: "research"',
    );
    expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
      "research?:",
    );
    expect(read("src/components/ai/hooks/useAssistantTasks.ts")).not.toContain(
      'kind: "research"',
    );
  });

  it("removes the dedicated research result message component", () => {
    expect(existsSync("src/components/ai/ResearchResultMessage.tsx")).toBe(
      false,
    );
    expect(() => read("src/components/ai/ResearchResultMessage.tsx")).toThrow();
  });

  it("keeps research output as normal markdown text in the assistant stream", () => {
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(tasks).toContain("result.summary.trim()");
    expect(read("src/components/ai/AiMessageList.tsx")).not.toContain(
      "artifactLinks",
    );
    expect(tasks).toContain("研究已完成，但没有生成可展示正文。");
    expect(read("src/components/ai/AiMessageList.tsx")).toContain(
      "AiMessageBubble",
    );
  });

  it("keeps the latest assistant markdown bubble in streaming mode after content appears", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "**正在生成**\n\n第一段内容",
            },
          ]}
          streaming={true}
        />,
      );
    });

    const bubble = document.body.querySelector(
      ".ai-message-bubble-assistant[data-streaming]",
    );

    expect(bubble).not.toBeNull();
    expect(document.body.textContent).toContain("正在生成");
    expect(document.body.textContent).toContain("第一段内容");
  });

  it("updates streaming assistant content when the message count is unchanged", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "initial token" }]}
          streaming={true}
        />,
      );
    });

    expect(document.body.textContent).toContain("initial token");

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "updated token" }]}
          streaming={true}
        />,
      );
    });

    expect(document.body.textContent).not.toContain("initial token");
    expect(document.body.textContent).toContain("updated token");
  });

  it("renders the complete final assistant content after streaming throttling", async () => {
    const finalContent = `起${"中".repeat(210)}最终内容`;

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "起" }]}
          streaming={true}
        />,
      );
    });

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: finalContent }]}
          streaming={true}
        />,
      );
    });

    expect(document.body.textContent).not.toContain("最终内容");

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: finalContent }]}
          streaming={false}
        />,
      );
    });

    expect(document.body.textContent).toContain("最终内容");
  });

  it("renders a research summary as a plain assistant markdown bubble", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "**研究摘要**\n\n- 普通文字流",
            },
          ]}
          streaming={false}
        />,
      );
    });

    expect(document.body.textContent).toContain("研究摘要");
    expect(document.body.textContent).toContain("普通文字流");
    expect(
      document.body.querySelector('[data-testid="assistant-artifact-tags"]'),
    ).toBeNull();
    expect(document.body.textContent).not.toContain("证据矩阵");
    expect(document.body.textContent).not.toContain("过程详情");
  });
});
