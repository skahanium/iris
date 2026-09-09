import { act, StrictMode } from "react";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { useAssistantConversationProjection } from "@/components/ai/hooks/useAssistantConversationProjection";
import { createAssistantRunEventState } from "@/lib/assistant-run-events";
import type { ChatLine } from "@/components/ai/AiMessageList";
vi.mock("@/lib/ipc", () => ({
  assistantSessionLoad: vi.fn().mockResolvedValue([]),
}));
it("ignores candidate output and late resets, publishing only one committed target", () => {
  let messages: ChatLine[] = [
    { role: "user", content: "问题", runId: "run" },
    { role: "assistant", content: "", runId: "run" },
  ];
  const host = document.createElement("div");
  const root = createRoot(host);
  const noop = () => undefined;
  function Probe({
    complete,
    epoch = 0,
    body = "正式答案",
  }: {
    complete: boolean;
    epoch?: number;
    body?: string;
  }) {
    useAssistantConversationProjection({
      run: {
        ...createAssistantRunEventState("run"),
        lastSeq: complete ? 5 : 3,
        state: complete ? "completed" : "running",
        content: body,
      },
      presentation: {
        runId: "run",
        lastSeq: epoch + 1,
        resetEpoch: epoch,
        resyncFromSeq: null,
        pendingEvents: [],
        processItems: [],
        answer: epoch ? "" : "候选草稿",
        answerComplete: false,
      },
      messages,
      setMessages: (u) => {
        messages = typeof u === "function" ? u(messages) : u;
      },
      setStreaming: noop,
      setActivityHint: noop,
      setError: noop,
    });
    return null;
  }
  try {
    act(() => root.render(<Probe complete={false} />));
    expect(messages[1]?.content).toBe("");
    act(() => root.render(<Probe complete />));
    expect(messages[1]?.content).toBe("正式答案");
    act(() => root.render(<Probe complete epoch={1} body="正式" />));
    expect(messages[1]?.content).toBe("正式答案");
    expect(messages[1]?.answerPresentation?.resetEpoch).toBe(0);
    act(() => root.render(<Probe complete epoch={2} body="另一个版本" />));
    expect(messages[1]?.content).toBe("正式答案");
  } finally {
    act(() => root.unmount());
  }
});

it("waits for the full durable body and never reports empty playback complete", () => {
  let messages: ChatLine[] = [
    { role: "user", content: "合成问题", runId: "gap" },
  ];
  const host = document.createElement("div");
  const root = createRoot(host);
  const noop = () => undefined;
  function Probe({ body, gap }: { body: string; gap: number | null }) {
    useAssistantConversationProjection({
      run: {
        ...createAssistantRunEventState("gap"),
        state: "completed",
        lastSeq: 5,
        content: body,
        resyncFromSeq: gap,
      },
      messages,
      setMessages: (u) => {
        messages = typeof u === "function" ? u(messages) : u;
      },
      setStreaming: noop,
      setActivityHint: noop,
      setError: noop,
    });
    return null;
  }
  try {
    act(() => root.render(<Probe body="" gap={null} />));
    expect(messages[1]?.answerPresentation?.complete).toBe(false);
    expect(
      messages[1]?.processItems?.some((item) => item.label === "答复完毕"),
    ).toBe(false);
    act(() => root.render(<Probe body="部分" gap={3} />));
    expect(messages[1]?.content).toBe("");
    act(() => root.render(<Probe body="完整正文。" gap={null} />));
    expect(messages[1]?.content).toBe("完整正文。");
    expect(messages[1]?.answerPresentation?.complete).toBe(true);
  } finally {
    act(() => root.unmount());
  }
});

it("takes a classified result once under StrictMode and publishes it only when available", async () => {
  let messages: ChatLine[] = [
    { role: "user", content: "合成涉密问题", runId: "classified" },
  ];
  let resolve!: (value: string) => void;
  const take = vi.fn(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  const host = document.createElement("div");
  const root = createRoot(host);
  const noop = () => undefined;
  const setMessages: React.Dispatch<React.SetStateAction<ChatLine[]>> = (u) => {
    messages = typeof u === "function" ? u(messages) : u;
  };
  function Probe() {
    useAssistantConversationProjection({
      run: {
        ...createAssistantRunEventState("classified"),
        state: "completed",
        lastSeq: 5,
        content: "",
      },
      classifiedContextRef: "test-context",
      takeClassifiedResult: take,
      messages,
      setMessages,
      setStreaming: noop,
      setActivityHint: noop,
      setError: noop,
    });
    return null;
  }
  try {
    act(() =>
      root.render(
        <StrictMode>
          <Probe />
        </StrictMode>,
      ),
    );
    expect(take).toHaveBeenCalledTimes(1);
    expect(messages[1]?.content).toBe("");
    expect(messages[1]?.answerPresentation?.complete).toBe(false);
    await act(async () => {
      resolve("合成内存答复。");
    });
    expect(messages[1]?.content).toBe("合成内存答复。");
    expect(messages[1]?.answerPresentation?.complete).toBe(true);
  } finally {
    act(() => root.unmount());
  }
});

it("reports playback timing once per confirmed body", () => {
  const host = document.createElement("div");
  const root = createRoot(host);
  const records: Record<string, unknown>[] = [];
  host.addEventListener("iris-answer-presented", (event) =>
    records.push((event as CustomEvent<Record<string, unknown>>).detail),
  );
  const render = () => (
    <AiMessageBubble
      role="assistant"
      content="合成终稿。"
      messageIdentity="timing"
      answerPresentation={{
        runId: "timing",
        resetEpoch: 0,
        complete: true,
        settled: true,
      }}
    />
  );
  try {
    act(() => root.render(render()));
    act(() => root.render(render()));
    expect(records).toHaveLength(1);
    expect(records[0]?.readyToFirstVisibleMs).toEqual(expect.any(Number));
    expect(records[0]?.playbackMs).toEqual(expect.any(Number));
  } finally {
    act(() => root.unmount());
  }
});
