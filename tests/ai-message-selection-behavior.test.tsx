import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageList } from "@/components/ai/AiMessageList";
import { ConversationSurface } from "@/components/ai/ConversationSurface";

const { toast } = vi.hoisted(() => ({ toast: vi.fn() }));

vi.mock("@/components/ui/use-toast", () => ({
  useToast: () => toast,
}));

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

describe("AI message selection behavior", () => {
  let host: HTMLDivElement;
  let root: Root;
  const writeText = vi.fn<Clipboard["writeText"]>();

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    writeText.mockReset();
    writeText.mockResolvedValue(undefined);
    toast.mockReset();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("keeps body clicks for text selection and selects messages from the checkbox control", async () => {
    const onSelect = vi.fn();

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            { role: "assistant", content: "可复制的局部文字" },
            { role: "user", content: "用户消息也可以勾选" },
          ]}
          streaming={false}
          selectedIndices={new Set()}
          onSelect={onSelect}
        />,
      );
    });

    const body = host.querySelector(".ai-message-body");
    expect(body).not.toBeNull();

    await act(async () => {
      body?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onSelect).not.toHaveBeenCalled();

    const selectButton = host.querySelector<HTMLButtonElement>(
      'button[aria-label="选择此消息"]',
    );
    expect(selectButton).not.toBeNull();

    await act(async () => {
      selectButton?.dispatchEvent(
        new MouseEvent("click", { bubbles: true, shiftKey: true }),
      );
    });

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(0, {
      shiftKey: true,
      metaKey: false,
      ctrlKey: false,
    });
  });

  it("keeps message action controls outside the message bubble content layer", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "正文不应被按钮遮挡" }]}
          streaming={false}
          selectedIndices={new Set([0])}
          onSelect={vi.fn()}
          onRetract={vi.fn()}
        />,
      );
    });

    const bubble = host.querySelector(".ai-message-bubble");
    expect(bubble).not.toBeNull();
    expect(
      bubble?.querySelector('button[aria-label="取消选择此消息"]'),
    ).toBeNull();
    expect(bubble?.querySelector('button[title="复制此消息"]')).toBeNull();
    expect(
      bubble?.querySelector('button[title="撤回此消息及后续所有消息"]'),
    ).toBeNull();

    expect(
      host.querySelector('button[aria-label="取消选择此消息"]'),
    ).not.toBeNull();
    expect(host.querySelector('button[title="复制此消息"]')).not.toBeNull();
    expect(
      host.querySelector('button[title="撤回此消息及后续所有消息"]'),
    ).not.toBeNull();
  });

  it("renders process events in the assistant bubble without copying them as answer text", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "最终正文",
              processItems: [
                {
                  id: "stage:1",
                  kind: "stage",
                  label: "正在检索笔记",
                  status: "completed",
                  createdAt: 1,
                },
              ],
            },
          ]}
          streaming={true}
          selectedIndices={new Set([0])}
          onSelect={vi.fn()}
        />,
      );
    });

    expect(host.textContent).toContain("处理过程");
    expect(host.textContent).toContain("正在检索笔记");

    const copyButton = host.querySelector<HTMLButtonElement>(
      'button[title="复制此消息"]',
    );
    expect(copyButton).not.toBeNull();

    await act(async () => {
      copyButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(writeText).toHaveBeenCalledWith("最终正文");
    expect(writeText).not.toHaveBeenCalledWith(
      expect.stringContaining("正在检索笔记"),
    );
    expect(toast).toHaveBeenCalledWith("已复制回答", { tone: "success" });
  });

  it("shows the latest process step only in the collapsed timeline header", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "",
              processItems: [
                {
                  id: "stage:1",
                  kind: "stage",
                  label: "联网检索中...",
                  status: "completed",
                  createdAt: 1,
                },
                {
                  id: "stage:2",
                  kind: "stage",
                  label: "chat完成。",
                  status: "completed",
                  createdAt: 2,
                },
              ],
            },
          ]}
          streaming={true}
          selectedIndices={new Set()}
          onSelect={vi.fn()}
        />,
      );
    });

    const timeline = host.querySelector<HTMLElement>(
      '[data-testid="assistant-process-timeline"]',
    );
    const toggle = timeline?.querySelector<HTMLButtonElement>("button");
    expect(timeline).not.toBeNull();
    expect(toggle).not.toBeNull();
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    expect(toggle?.textContent).toBe("处理过程");
    expect(timeline?.textContent).toContain("联网检索中...");
    expect(timeline?.textContent).toContain("chat完成。");

    await act(async () => {
      toggle?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(toggle?.textContent).toContain("处理过程");
    expect(toggle?.textContent).toContain("chat完成。");
    expect(timeline?.textContent).not.toContain("联网检索中...");
  });

  it("keeps every bounded process item available while the timeline is expanded", async () => {
    const processItems = Array.from({ length: 9 }, (_, index) => ({
      id: `stage:${index + 1}`,
      kind: "stage" as const,
      label: `处理步骤 ${index + 1}`,
      status: "completed" as const,
      createdAt: index + 1,
    }));

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "", processItems }]}
          streaming
        />,
      );
    });

    const timeline = host.querySelector<HTMLElement>(
      '[data-testid="assistant-process-timeline"]',
    );
    expect(timeline?.textContent).toContain("处理步骤 1");
    expect(timeline?.textContent).toContain("处理步骤 9");
  });

  it("hides the standalone thinking indicator when process events are visible", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            { role: "user", content: "MOE架构是什么意思？" },
            {
              role: "assistant",
              content: "",
              processItems: [
                {
                  id: "stage:1",
                  kind: "stage",
                  label: "正在流式输出最终回答...",
                  status: "completed",
                  createdAt: 1,
                },
              ],
            },
          ]}
          streaming={true}
          selectedIndices={new Set()}
          onSelect={vi.fn()}
        />,
      );
    });

    expect(host.textContent).toContain("处理过程");
    expect(host.textContent).toContain("正在流式输出最终回答...");
    expect(host.textContent).not.toContain("正在思考");
  });

  it("collapses the live process exactly once when final output starts", async () => {
    const processItems = [
      {
        id: "reasoning:1",
        kind: "reasoning_summary" as const,
        label: "先核验来源，再组织答案。",
        status: "completed" as const,
        createdAt: 1,
      },
    ];

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "", processItems }]}
          streaming
        />,
      );
    });
    const toggle = host.querySelector<HTMLButtonElement>(
      '[data-testid="assistant-process-timeline"] button',
    );
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "最终正文", processItems }]}
          streaming
        />,
      );
    });
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");

    await act(async () => {
      toggle?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      root.render(
        <AiMessageList
          messages={[
            { role: "assistant", content: "最终正文继续", processItems },
          ]}
          streaming
        />,
      );
    });
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
  });

  it("copies the context-menu selection snapshot even if the DOM selection is cleared", async () => {
    const messageListRef = { current: null as HTMLDivElement | null };
    const onQuoteToInput = vi.fn();

    await act(async () => {
      root.render(
        <ConversationSurface
          messages={[{ role: "assistant", content: "复制这一段文字" }]}
          streaming={false}
          selectedIndices={new Set()}
          messageListRef={messageListRef}
          onCitationClick={vi.fn()}
          onQuoteToInput={onQuoteToInput}
          onSelect={vi.fn()}
        />,
      );
    });

    const body = host.querySelector(".ai-message-body");
    expect(body?.firstChild).toBeTruthy();
    const range = document.createRange();
    range.selectNodeContents(body!.firstChild!);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    await act(async () => {
      body?.dispatchEvent(
        new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: 12,
          clientY: 12,
        }),
      );
    });

    selection?.removeAllRanges();
    const copyItem = document.getElementById("ctx-copy");
    expect(copyItem).not.toBeNull();

    await act(async () => {
      copyItem?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(writeText).toHaveBeenCalledWith("复制这一段文字");
  });

  it("does not clear the native text selection after context-menu copy", async () => {
    const messageListRef = { current: null as HTMLDivElement | null };

    await act(async () => {
      root.render(
        <ConversationSurface
          messages={[{ role: "assistant", content: "保持选区文字" }]}
          streaming={false}
          selectedIndices={new Set()}
          messageListRef={messageListRef}
          onCitationClick={vi.fn()}
          onQuoteToInput={vi.fn()}
          onSelect={vi.fn()}
        />,
      );
    });

    const body = host.querySelector(".ai-message-body");
    expect(body?.firstChild).toBeTruthy();
    const range = document.createRange();
    range.selectNodeContents(body!.firstChild!);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    await act(async () => {
      body?.dispatchEvent(
        new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: 12,
          clientY: 12,
        }),
      );
    });

    const copyItem = document.getElementById("ctx-copy");
    expect(copyItem).not.toBeNull();

    await act(async () => {
      copyItem?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(writeText).toHaveBeenCalledWith("保持选区文字");
    expect(window.getSelection()?.toString()).toBe("保持选区文字");
  });

  it("copies AI message text selection with the keyboard shortcut", async () => {
    const messageListRef = { current: null as HTMLDivElement | null };

    await act(async () => {
      root.render(
        <ConversationSurface
          messages={[{ role: "assistant", content: "快捷键复制文字" }]}
          streaming={false}
          selectedIndices={new Set()}
          messageListRef={messageListRef}
          onCitationClick={vi.fn()}
          onQuoteToInput={vi.fn()}
          onSelect={vi.fn()}
        />,
      );
    });

    const body = host.querySelector(".ai-message-body");
    expect(body?.firstChild).toBeTruthy();
    const range = document.createRange();
    range.selectNodeContents(body!.firstChild!);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    await act(async () => {
      document.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "c",
          bubbles: true,
          ctrlKey: true,
        }),
      );
    });

    expect(writeText).toHaveBeenCalledWith("快捷键复制文字");
  });
});
