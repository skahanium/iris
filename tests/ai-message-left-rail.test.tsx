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

describe("AI message left rail layout contract", () => {
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

  describe("select/copy/retract controls are outside .ai-message-bubble", () => {
    it("select control is outside the bubble for user messages", async () => {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "user", content: "用户消息" }]}
            streaming={false}
            selectedIndices={new Set([0])}
            onSelect={vi.fn()}
          />,
        );
      });

      const bubble = host.querySelector(".ai-message-bubble");
      expect(bubble).not.toBeNull();
      // Select button must be outside the bubble
      expect(
        bubble?.querySelector('button[aria-label="取消选择此消息"]'),
      ).toBeNull();
      // But present in the DOM
      expect(
        host.querySelector('button[aria-label="取消选择此消息"]'),
      ).not.toBeNull();
    });

    it("select control is outside the bubble for assistant messages", async () => {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "助手消息" }]}
            streaming={false}
            selectedIndices={new Set([0])}
            onSelect={vi.fn()}
          />,
        );
      });

      const bubble = host.querySelector(".ai-message-bubble");
      expect(bubble).not.toBeNull();
      expect(
        bubble?.querySelector('button[aria-label="取消选择此消息"]'),
      ).toBeNull();
      expect(
        host.querySelector('button[aria-label="取消选择此消息"]'),
      ).not.toBeNull();
    });

    it("copy and retract controls are outside the bubble for assistant messages", async () => {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "可复制内容" }]}
            streaming={false}
            selectedIndices={new Set()}
            onSelect={vi.fn()}
            onRetract={vi.fn()}
          />,
        );
      });

      const bubble = host.querySelector(".ai-message-bubble");
      expect(bubble).not.toBeNull();
      expect(bubble?.querySelector('button[title="复制此消息"]')).toBeNull();
      expect(
        bubble?.querySelector('button[title="撤回此消息及后续所有消息"]'),
      ).toBeNull();

      // But present outside the bubble
      expect(host.querySelector('button[title="复制此消息"]')).not.toBeNull();
      expect(
        host.querySelector('button[title="撤回此消息及后续所有消息"]'),
      ).not.toBeNull();
    });
  });

  describe("assistant rows use left rail layout without right action rail", () => {
    it("assistant row uses left-rail plus content layout (no right actions column)", async () => {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "测试消息" }]}
            streaming={false}
            selectedIndices={new Set()}
            onSelect={vi.fn()}
            onRetract={vi.fn()}
          />,
        );
      });

      // Contract: actions (select/copy/retract) must be in a LEFT rail, not right
      // The old layout had grid-cols-[1.75rem_minmax(0,1fr)_3.5rem] with right actions
      // The new layout should NOT have a right-side actions column
      const rightActions = host.querySelector(".flex.justify-end.pt-1");
      expect(rightActions).toBeNull();

      // Actions must be present in a left-side rail
      const selectButton = host.querySelector(
        'button[aria-label="选择此消息"]',
      );
      expect(selectButton).not.toBeNull();
    });

    it("assistant message actions do not render during streaming", async () => {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "" }]}
            streaming={true}
            selectedIndices={new Set()}
            onSelect={vi.fn()}
            onRetract={vi.fn()}
          />,
        );
      });

      // Copy and retract buttons should not be present during streaming
      expect(host.querySelector('button[title="复制此消息"]')).toBeNull();
      expect(
        host.querySelector('button[title="撤回此消息及后续所有消息"]'),
      ).toBeNull();
    });
  });

  describe("ai-message-body text selection does not trigger message select", () => {
    it("clicking on message body does not call onSelect", async () => {
      const onSelect = vi.fn();

      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "可选文字内容" }]}
            streaming={false}
            selectedIndices={new Set()}
            onSelect={onSelect}
          />,
        );
      });

      const body = host.querySelector(".ai-message-body");
      expect(body).not.toBeNull();

      // Click on the body text should NOT trigger message selection
      await act(async () => {
        body?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      });

      expect(onSelect).not.toHaveBeenCalled();
    });

    it("only the dedicated select button triggers onSelect", async () => {
      const onSelect = vi.fn();

      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content: "按钮选择" }]}
            streaming={false}
            selectedIndices={new Set()}
            onSelect={onSelect}
          />,
        );
      });

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
  });
});
