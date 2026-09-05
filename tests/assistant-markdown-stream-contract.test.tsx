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

describe("assistant Run transcript rendering", () => {
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

  it("uses the single conversation projection instead of competing transcript hooks", () => {
    const list = read("src/components/ai/AiMessageList.tsx");
    const projection = read(
      "src/components/ai/hooks/useAssistantConversationProjection.ts",
    );

    expect(existsSync("src/components/ai/ResearchResultMessage.tsx")).toBe(
      false,
    );
    expect(existsSync("src/components/ai/hooks/useAssistantTasks.ts")).toBe(
      false,
    );
    expect(list).not.toContain("ResearchResultMessage");
    expect(projection).toContain("run.content");
    expect(projection).toContain('run.state === "completed"');
    expect(
      existsSync("src/components/ai/hooks/useAssistantRunTranscript.ts"),
    ).toBe(false);
    expect(
      existsSync("src/components/ai/hooks/useAssistantPresentationPlayback.ts"),
    ).toBe(false);
  });

  it("renders the current assistant bubble while a Run is streaming", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "initial Run fragment" }]}
          streaming={true}
        />,
      );
    });

    expect(document.body.textContent).toContain("initial Run fragment");
    expect(
      document.body.querySelector(
        ".ai-message-bubble-assistant[data-streaming]",
      ),
    ).not.toBeNull();
  });

  it("updates the final assistant bubble without requiring a second message", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "first durable delta" }]}
          streaming={true}
        />,
      );
    });

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "assistant",
              content: "first durable delta plus final content",
            },
          ]}
          streaming={false}
        />,
      );
    });

    expect(document.body.textContent).toContain("final content");
  });

  it("renders required input inside the owning conversation turn", async () => {
    const submit = vi.fn();
    const cancel = vi.fn();

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "user",
              content: "今晚附近影院有什么场次？",
              runId: "run-location",
            },
            { role: "assistant", content: "", runId: "run-location" },
          ]}
          streaming={false}
          pendingInput={{
            runId: "run-location",
            prompt: "要查询附近影院或场次，请告诉我城市。",
            fields: ["city"],
            values: { city: "佛山" },
            submitting: false,
            onValueChange: vi.fn(),
            onSubmit: submit,
            onCancel: cancel,
          }}
        />,
      );
    });

    const card = document.body.querySelector(
      '[data-testid="assistant-run-input-required"]',
    );
    expect(card).not.toBeNull();
    expect(
      document.body.textContent?.indexOf("今晚附近影院有什么场次？"),
    ).toBeLessThan(
      document.body.textContent?.indexOf("要查询附近影院或场次") ?? -1,
    );

    const buttons = Array.from(card?.querySelectorAll("button") ?? []);
    act(() => buttons.find((button) => button.textContent === "继续")?.click());
    act(() =>
      buttons.find((button) => button.textContent === "取消本轮")?.click(),
    );
    expect(submit).toHaveBeenCalledOnce();
    expect(cancel).toHaveBeenCalledOnce();
  });

  it("marks a historical cancelled user-only turn", async () => {
    await act(async () => {
      root.render(
        <AiMessageList
          messages={[
            {
              role: "user",
              content: "最近有什么新上映的电影吗？",
              runId: "run-cancelled",
              turnState: "cancelled",
            },
          ]}
          streaming={false}
        />,
      );
    });

    expect(document.body.textContent).toContain("本次回答已取消");
  });
});
