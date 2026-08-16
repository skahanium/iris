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
});
