import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { assistantMessageIdentity } from "@/lib/ai-message-identity";
import { createAssistantRunEventState } from "@/lib/assistant-run-events";
import { useAssistantConversationProjection } from "@/components/ai/hooks/useAssistantConversationProjection";
import type { ChatLine } from "@/components/ai/AiMessageList";

const { loadSession } = vi.hoisted(() => ({ loadSession: vi.fn() }));
vi.mock("@/lib/ipc", () => ({ assistantSessionLoad: loadSession }));

describe("stream lifecycle continuity", () => {
  it("plays inside the answer without rendering its parent on every frame", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    let next = 0;
    const raf = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((cb) => {
        callbacks.set(++next, cb);
        return next;
      });
    const cancel = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((id) => {
        callbacks.delete(id);
      });
    let parentRenders = 0;
    function Parent() {
      parentRenders += 1;
      return (
        <AiMessageBubble
          role="assistant"
          content={"连续回答".repeat(100)}
          messageIdentity="local-run"
          answerPresentation={{
            runId: "local-run",
            resetEpoch: 0,
            complete: true,
          }}
        />
      );
    }
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    try {
      act(() => root.render(<Parent />));
      expect(host.querySelector(".ai-message-body")?.textContent ?? "").toBe(
        "",
      );
      const initialRenders = parentRenders;
      for (let frame = 1; frame <= 30; frame += 1) {
        const batch = [...callbacks.values()];
        callbacks.clear();
        act(() => batch.forEach((cb) => cb((frame * 1000) / 60)));
      }
      expect(
        host.querySelector(".ai-message-body")?.textContent?.length,
      ).toBeGreaterThan(0);
      expect(parentRenders).toBe(initialRenders);
      const showAll = host.querySelector<HTMLButtonElement>(
        '[aria-label="立即显示全部"]',
      );
      expect(showAll).not.toBeNull();
      act(() => showAll?.click());
      expect(host.querySelector(".ai-message-body")?.textContent).toContain(
        "连续回答".repeat(100),
      );
    } finally {
      act(() => root.unmount());
      host.remove();
      raf.mockRestore();
      cancel.mockRestore();
    }
  });

  it("keeps the outgoing user identity when intake binds durable metadata", () => {
    const before = { role: "user", clientRequestId: "request" };
    expect(
      assistantMessageIdentity({ ...before, runId: "run", turnId: "turn" }, 0),
    ).toBe(assistantMessageIdentity(before, 0));
  });

  it("does not remount an assistant when turn metadata is hydrated", () => {
    const before = { role: "assistant", runId: "run" };
    expect(assistantMessageIdentity({ ...before, turnId: "turn" }, 1)).toBe(
      assistantMessageIdentity(before, 1),
    );
  });

  it("projects complete authoritative targets into local playback, ignoring stale reset and stop", () => {
    let messages: ChatLine[] = [
      { role: "user", content: "问题", runId: "run" },
      { role: "assistant", content: "", runId: "run" },
    ];
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const noop = () => undefined;
    function Probe({
      state,
      epoch = 0,
      answer = "完整",
    }: {
      state: "running" | "completed" | "cancelled";
      epoch?: number;
      answer?: string;
    }) {
      useAssistantConversationProjection({
        run: {
          ...createAssistantRunEventState("run"),
          state,
          lastSeq: state === "running" ? 2 : 4,
          content: state === "completed" ? "完整回答" : "",
        },
        presentation: {
          runId: "run",
          lastSeq: 2 + epoch,
          resetEpoch: epoch,
          resyncFromSeq: null,
          pendingEvents: [],
          processItems: [],
          answer,
          answerComplete: false,
        },
        messages,
        setMessages: (update) => {
          messages = typeof update === "function" ? update(messages) : update;
        },
        setStreaming: noop,
        setActivityHint: noop,
        setError: noop,
      });
      return null;
    }
    try {
      act(() => root.render(<Probe state="running" />));
      expect(messages[1]?.content).toBe("");
      expect(messages[1]?.answerPresentation).toMatchObject({
        runId: "run",
        resetEpoch: 0,
        complete: false,
      });
      act(() => root.render(<Probe state="completed" />));
      expect(messages[1]?.content).toBe("完整回答");
      expect(messages[1]?.answerPresentation?.complete).toBe(true);
      act(() => root.render(<Probe state="running" epoch={1} answer="" />));
      expect(messages[1]?.content).toBe("完整回答");
      expect(messages[1]?.answerPresentation?.resetEpoch).toBe(0);
      act(() =>
        root.render(<Probe state="cancelled" epoch={1} answer="安全前缀" />),
      );
      expect(messages[1]?.answerPresentation?.stopped).toBe(false);
    } finally {
      act(() => root.unmount());
      host.remove();
    }
  });

  it("does not let delayed hydration shrink an accepted answer target", async () => {
    loadSession.mockResolvedValue([
      { role: "assistant", runId: "hydrated", content: "完整", seq: 2 },
    ]);
    let messages: ChatLine[] = [
      { role: "user", content: "问题", runId: "hydrated" },
      {
        role: "assistant",
        content: "完整答案",
        runId: "hydrated",
        answerPresentation: {
          runId: "hydrated",
          resetEpoch: 0,
          complete: true,
        },
      },
    ];
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const noop = () => undefined;
    function Probe() {
      useAssistantConversationProjection({
        run: {
          ...createAssistantRunEventState("hydrated"),
          lastSeq: 3,
          state: "completed",
          content: "完整答案",
        },
        session: { domain: "normal", sessionKey: "synthetic" },
        messages,
        setMessages: (update) => {
          messages = typeof update === "function" ? update(messages) : update;
        },
        setStreaming: noop,
        setActivityHint: noop,
        setError: noop,
      });
      return null;
    }
    try {
      await act(async () => root.render(<Probe />));
      expect(messages[1]?.content).toBe("完整答案");
    } finally {
      act(() => root.unmount());
      host.remove();
    }
  });

  it("never flashes the full answer when durable completion arrives first", () => {
    let messages: ChatLine[] = [
      { role: "user", content: "合成问题", runId: "run" },
      { role: "assistant", content: "", runId: "run" },
    ];
    const noop = () => undefined;
    const setMessages = (update: React.SetStateAction<ChatLine[]>) => {
      messages = typeof update === "function" ? update(messages) : update;
    };
    function Probe({
      completed,
      presentationComplete,
    }: {
      completed: boolean;
      presentationComplete: boolean;
    }) {
      useAssistantConversationProjection({
        run: {
          ...createAssistantRunEventState("run"),
          state: completed ? "completed" : "running",
          lastSeq: completed ? 3 : 2,
          content: "完整回答的剩余内容",
        },
        presentation: {
          runId: "run",
          lastSeq: presentationComplete ? 4 : 2,
          resyncFromSeq: null,
          pendingEvents: [],
          processItems: [],
          answer: "完整回答的剩余内容",
          answerComplete: presentationComplete,
        },
        presentationReveal: { runId: "run", answer: "完整", revealing: true },
        messages,
        setMessages,
        setStreaming: noop,
        setActivityHint: noop,
        setError: noop,
      });
      return null;
    }
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    try {
      for (const [completed, presentationComplete] of [
        [false, false],
        [true, false],
        [true, true],
      ]) {
        act(() =>
          root.render(
            createElement(Probe, {
              completed: completed!,
              presentationComplete: presentationComplete!,
            }),
          ),
        );
        expect(messages[1]?.content).toBe(
          completed ? "完整回答的剩余内容" : "",
        );
        expect(messages[1]?.answerPresentation?.complete).toBe(completed);
        expect(messages[1]?.presentationStreaming).toBe(!completed);
      }
    } finally {
      act(() => root.unmount());
      host.remove();
    }
  });
});
