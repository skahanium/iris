import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AiMessageList, type ChatLine } from "@/components/ai/AiMessageList";

const windowState = vi.hoisted(() => ({ visible: [0] }));
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: () => ({
    getVirtualItems: () =>
      windowState.visible.map((index) => ({
        index,
        start: index * 100,
        size: 100,
      })),
    measureElement: vi.fn(),
  }),
}));
vi.mock("@/components/ai/AiMessageBubble", () => ({
  AiMessageBubble: ({
    content,
    answerPresentation,
  }: {
    content?: string;
    answerPresentation?: { complete?: boolean; initialVisibleLength?: number };
  }) => (
    <div
      className="ai-message-bubble-assistant"
      data-presentation-phase={
        answerPresentation?.complete ? "complete" : "receiving"
      }
      data-resumed-length={answerPresentation?.initialVisibleLength ?? 0}
    >
      {content}
    </div>
  ),
}));

describe("completed playback across message windows", () => {
  let host: HTMLDivElement | undefined;
  let root: Root | undefined;
  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    windowState.visible = [0];
  });

  it("does not revive an offscreen completed run and resumes its visible prefix on return", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const history: ChatLine[] = Array.from({ length: 50 }, (_, index) => ({
      role: "assistant",
      content: `history${index}`,
      runId: `old${index}`,
    }));
    const answer: ChatLine = {
      role: "assistant",
      content: "completed answer",
      runId: "live",
      answerPresentation: { runId: "live", resetEpoch: 0, complete: false },
    };
    act(() =>
      root?.render(<AiMessageList messages={[...history, answer]} streaming />),
    );
    expect(
      host.querySelector(
        '[data-conversation-row="50"] .ai-message-bubble-assistant',
      ),
    ).not.toBeNull();
    const completed = {
      ...answer,
      answerPresentation: { ...answer.answerPresentation!, complete: true },
    };
    act(() => {
      root?.render(
        <AiMessageList messages={[...history, completed]} streaming={false} />,
      );
    });
    act(() =>
      host?.querySelector("[data-radix-scroll-area-viewport]")?.dispatchEvent(
        new CustomEvent("iris-answer-presented", {
          bubbles: true,
          detail: {
            runId: "live",
            resetEpoch: 0,
            length: answer.content.length,
          },
        }),
      ),
    );
    act(() =>
      root?.render(
        <AiMessageList messages={[...history, completed]} streaming={false} />,
      ),
    );
    expect(
      host.querySelector(
        '[data-conversation-row="50"] .ai-message-bubble-assistant',
      ),
    ).toBeNull();
    act(() =>
      root?.render(
        <AiMessageList messages={[...history, completed]} streaming={false} />,
      ),
    );
    expect(
      host.querySelector(
        '[data-conversation-row="50"] .ai-message-bubble-assistant',
      ),
    ).toBeNull();
    windowState.visible = [0, 50];
    act(() =>
      root?.render(
        <AiMessageList messages={[...history, completed]} streaming={false} />,
      ),
    );
    expect(
      host
        .querySelector('[data-conversation-row="50"] [data-resumed-length]')
        ?.getAttribute("data-resumed-length"),
    ).toBe(String(answer.content.length));
  });
  it("retains the stopped prefix and isolates a later reset epoch", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const history: ChatLine[] = Array.from({ length: 50 }, (_, index) => ({
      role: "assistant",
      content: `history${index}`,
      runId: `old${index}`,
    }));
    const answer: ChatLine = {
      role: "assistant",
      content: "partially visible answer",
      runId: "stopped",
      answerPresentation: {
        runId: "stopped",
        resetEpoch: 0,
        complete: false,
        stopped: true,
      },
    };
    act(() =>
      root?.render(
        <AiMessageList messages={[...history, answer]} streaming={false} />,
      ),
    );
    act(() =>
      host?.querySelector("[data-radix-scroll-area-viewport]")?.dispatchEvent(
        new CustomEvent("iris-answer-presented", {
          bubbles: true,
          detail: { runId: "stopped", resetEpoch: 0, length: 9 },
        }),
      ),
    );
    expect(
      host.querySelector(
        '[data-conversation-row="50"] .ai-message-bubble-assistant',
      ),
    ).toBeNull();
    windowState.visible = [0, 50];
    act(() =>
      root?.render(
        <AiMessageList messages={[...history, answer]} streaming={false} />,
      ),
    );
    expect(
      host
        .querySelector('[data-conversation-row="50"] [data-resumed-length]')
        ?.getAttribute("data-resumed-length"),
    ).toBe("9");
    const reset = {
      ...answer,
      answerPresentation: {
        ...answer.answerPresentation!,
        resetEpoch: 1,
        stopped: false,
      },
    };
    act(() =>
      root?.render(<AiMessageList messages={[...history, reset]} streaming />),
    );
    expect(
      host
        .querySelector('[data-conversation-row="50"] [data-resumed-length]')
        ?.getAttribute("data-resumed-length"),
    ).toBe("0");
  });
});
