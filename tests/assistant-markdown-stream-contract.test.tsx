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
