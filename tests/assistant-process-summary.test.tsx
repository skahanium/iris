import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageList } from "@/components/ai/AiMessageList";
import {
  ANSWER_COMPLETE_PROCESS_ID,
  ANSWER_COMPLETE_PROCESS_LABEL,
} from "@/lib/assistant-presentation";
import { ensureTerminalAnswerComplete } from "@/lib/ensure-answer-complete-process";

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

describe("assistant process fold summary", () => {
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

  it("折叠摘要显示末项答复完毕而非正在生成答复", async () => {
    const rawItems = [
      {
        id: "stage:3",
        kind: "stage" as const,
        label: "正在生成答复",
        status: "completed" as const,
        createdAt: 3,
      },
    ];
    const processItems = ensureTerminalAnswerComplete(rawItems, "completed");

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "最终正文",
              processItems,
            },
          ]}
          streaming={false}
        />,
      );
    });

    const timeline = host.querySelector(
      '[data-testid="assistant-process-timeline"]',
    );
    expect(timeline?.textContent).toContain(ANSWER_COMPLETE_PROCESS_LABEL);
    expect(timeline?.textContent).not.toMatch(/正在生成答复[^完]/);
    expect(processItems.at(-1)?.id).toBe(ANSWER_COMPLETE_PROCESS_ID);
  });
});
