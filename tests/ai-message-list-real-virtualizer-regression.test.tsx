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
});
