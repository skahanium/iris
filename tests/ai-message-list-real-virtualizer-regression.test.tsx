import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ErrorBoundary } from "@/components/ErrorBoundary";
import { AiMessageList } from "@/components/ai/AiMessageList";
import { ConversationSurface } from "@/components/ai/ConversationSurface";

describe("AiMessageList real virtualizer regression", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    host.style.height = "640px";
    host.style.width = "420px";
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("does not enter a nested update loop while streaming a non-empty assistant message", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "第一段" }]}
          streaming={true}
        />,
      );
    });

    for (const content of [
      "第一段\n\n第二段",
      "第一段\n\n第二段\n\n```ts\nconst x = 1;\n```",
      "第一段\n\n第二段\n\n```ts\nconst x = 1;\n```\n\n| A | B |\n| - | - |\n| 1 | 2 |",
    ]) {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content }]}
            streaming={true}
          />,
        );
      });
    }

    expect(
      consoleError.mock.calls
        .flat()
        .some((entry) =>
          String(entry).includes("Maximum update depth exceeded"),
        ),
    ).toBe(false);
  });
  it("keeps the conversation surface mounted through a short date-question stream", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const messageListRef = {
      current: null,
    } as React.RefObject<HTMLDivElement | null>;
    const renderConversation = (
      messages: Parameters<typeof ConversationSurface>[0]["messages"],
      streaming: boolean,
    ) =>
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ConversationSurface
            messages={messages}
            streaming={streaming}
            messageListRef={messageListRef}
            onCitationClick={() => undefined}
            onQuoteToInput={() => undefined}
          />
        </ErrorBoundary>,
      );

    await act(async () => {
      renderConversation([{ role: "user", content: "今天是几月几日？" }], true);
    });
    await act(async () => {
      renderConversation(
        [
          { role: "user", content: "今天是几月几日？" },
          { role: "assistant", content: "" },
        ],
        true,
      );
    });
    await act(async () => {
      renderConversation(
        [
          { role: "user", content: "今天是几月几日？" },
          { role: "assistant", content: "今天是 2026 年 6 月 29 日。" },
        ],
        true,
      );
    });

    expect(document.body.textContent).not.toContain("界面出现异常");
    expect(
      consoleError.mock.calls
        .flat()
        .some((entry) =>
          String(entry).includes("Maximum update depth exceeded"),
        ),
    ).toBe(false);
  });

  it("keeps the conversation surface mounted through a rich markdown stream", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const messageListRef = {
      current: null,
    } as React.RefObject<HTMLDivElement | null>;
    const renderConversation = (answer: string, streaming: boolean) =>
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ConversationSurface
            messages={[
              {
                role: "user",
                content: "听说 glm5.2 的性能几乎不弱于 opus 4.8，是真的吗？",
              },
              { role: "assistant", content: answer },
            ]}
            streaming={streaming}
            messageListRef={messageListRef}
            onCitationClick={() => undefined}
            onQuoteToInput={() => undefined}
          />
        </ErrorBoundary>,
      );
    const streamFrames = [
      "基本属实，但需要分场景来看。",
      "基本属实，但需要分场景来看。\n\n## 差距被拉到 1% 以内的场景\n\n根据公开评测，GLM-5.2 在部分长上下文和工具使用任务上已经非常接近 Opus 4.8 [C1][W2]。",
      "基本属实，但需要分场景来看。\n\n## 差距被拉到 1% 以内的场景\n\n根据公开评测，GLM-5.2 在部分长上下文和工具使用任务上已经非常接近 Opus 4.8 [C1][W2]。\n\n| 评测基准 | GLM-5.2 vs Opus 4.8 |\n| --- | --- |\n| FrontierSWE | 仅低 1% [C1][W2] |\n| MCP-Atlas | 仅低 0.8% [C1] |\n| Code Arena | 全球可用模型第一 [C1] |",
      "基本属实，但需要分场景来看。\n\n## 差距被拉到 1% 以内的场景\n\n根据公开评测，GLM-5.2 在部分长上下文和工具使用任务上已经非常接近 Opus 4.8 [C1][W2]。\n\n| 评测基准 | GLM-5.2 vs Opus 4.8 |\n| --- | --- |\n| FrontierSWE | 仅低 1% [C1][W2] |\n| MCP-Atlas | 仅低 0.8% [C1] |\n| Code Arena | 全球可用模型第一 [C1] |\n\n## 仍有明显差距的场景\n\n| 评测基准 | 结果 |\n| --- | --- |\n| SWE-Marathon | 低 13% |\n| Terminal-Bench 2.1 | 低 4% |\n\n一句话总结：简单问答看起来几乎同档，但长周期工程任务仍要分场景判断。",
    ];

    for (const frame of streamFrames) {
      await act(async () => {
        renderConversation(frame, true);
      });
    }
    await act(async () => {
      renderConversation(streamFrames.at(-1) ?? "", false);
    });

    expect(document.body.textContent).not.toContain("界面出现异常");
    expect(
      consoleError.mock.calls
        .flat()
        .some((entry) =>
          String(entry).includes("Maximum update depth exceeded"),
        ),
    ).toBe(false);
  });

  it("keeps the conversation surface mounted through a MCP-only high-evidence answer", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const messageListRef = {
      current: null,
    } as React.RefObject<HTMLDivElement | null>;
    const ddgSources = Array.from({ length: 23 }, (_, index) => {
      const citation = `[C${index + 1}]`;
      return `- ${citation} MCP search result ${index + 1}: https://example.com/source-${index + 1}`;
    }).join("\n");
    const answer = [
      "我用 MCP 搜索提供方检索后，整理出下面这些来源。",
      "",
      "## 来源概览",
      "",
      ddgSources,
      "",
      "## 初步结论",
      "",
      "这些来源显示同一个主题在多个页面中反复出现，但需要继续核对正文与发布时间。".repeat(
        12,
      ),
    ].join("\n");

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ConversationSurface
            messages={[
              {
                role: "user",
                content: "只用当前 MCP 搜索提供方查一下这个问题",
              },
              { role: "assistant", content: "" },
            ]}
            streaming
            messageListRef={messageListRef}
            onCitationClick={() => undefined}
            onQuoteToInput={() => undefined}
          />
        </ErrorBoundary>,
      );
    });

    for (const frame of [
      answer.slice(0, 160),
      answer.slice(0, 800),
      answer.slice(0, 1600),
      answer,
    ]) {
      await act(async () => {
        root.render(
          <ErrorBoundary scope="AI对话区">
            <ConversationSurface
              messages={[
                {
                  role: "user",
                  content: "只用当前 MCP 搜索提供方查一下这个问题",
                },
                { role: "assistant", content: frame },
              ]}
              streaming
              messageListRef={messageListRef}
              onCitationClick={() => undefined}
              onQuoteToInput={() => undefined}
            />
          </ErrorBoundary>,
        );
      });
    }

    expect(document.body.textContent).not.toContain("界面出现异常");
    expect(
      consoleError.mock.calls
        .flat()
        .some((entry) =>
          String(entry).includes("Maximum update depth exceeded"),
        ),
    ).toBe(false);
  });
});
