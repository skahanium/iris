// Test-only Vite entry: production components, synthetic text, no provider or IPC calls.
import { Profiler, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useAssistantConversationProjection } from "@/components/ai/hooks/useAssistantConversationProjection";
import {
  createAssistantRunEventState,
  type AssistantRunEventState,
} from "@/lib/assistant-run-events";
import {
  createAssistantPresentationState,
  type AssistantPresentationState,
} from "@/lib/assistant-presentation";
import { AiMessageList, type ChatLine } from "@/components/ai/AiMessageList";
import "@/styles/globals.css";
import "@/styles/markdown-prose.css";

const observerStats = { instances: 0, observed: 0 };
const NativeResizeObserver = window.ResizeObserver;
window.ResizeObserver = class implements ResizeObserver {
  private readonly native: ResizeObserver;
  private readonly nodes = new Set<Element>();
  private live = false;
  constructor(callback: ResizeObserverCallback) {
    this.native = new NativeResizeObserver(callback);
  }
  observe(target: Element, options?: ResizeObserverOptions) {
    if (!this.live) {
      observerStats.instances += 1;
      this.live = true;
    }
    if (!this.nodes.has(target)) {
      this.nodes.add(target);
      observerStats.observed += 1;
    }
    this.native.observe(target, options);
  }
  unobserve(target: Element) {
    if (this.nodes.delete(target)) observerStats.observed -= 1;
    if (this.live && this.nodes.size === 0) {
      observerStats.instances -= 1;
      this.live = false;
    }
    this.native.unobserve(target);
  }
  disconnect() {
    observerStats.observed -= this.nodes.size;
    this.nodes.clear();
    if (this.live) {
      observerStats.instances -= 1;
      this.live = false;
    }
    this.native.disconnect();
  }
};
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    window.ResizeObserver = NativeResizeObserver;
  });

interface Sample {
  t: number;
  top: number;
  height: number;
  blocks: number;
  phase: string;
  observers: number;
  observedNodes: number;
  following: boolean;
}
interface Metrics {
  samples: Sample[];
  renderMs: number[];
  longTasks: number[];
  bodyReplacements: number;
  maxNodes: number;
  maxAnchorDrift: number;
}
const blankMetrics = (): Metrics => ({
  samples: [],
  renderMs: [],
  longTasks: [],
  bodyReplacements: 0,
  maxNodes: 0,
  maxAnchorDrift: 0,
});
const paragraphs =
  "# 合成流标题继续增长\n\n- 第一项\n\n- 第二项\n\n这是用于逐帧回放的合成段落。中英文混排 streaming 👨‍👩‍👧‍👦 é，检查换行与连续显示。\n\n```ts\nconst result = 42;\n```\n\n| 列一 | 列二 |\n| --- | --- |\n| 合成 | 数据 |\n\n";
const noop = () => undefined;
function App() {
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const [run, setRun] = useState<AssistantRunEventState | null>(null);
  const [presentation, setPresentation] =
    useState<AssistantPresentationState | null>(null);
  useAssistantConversationProjection({
    run,
    presentation,
    messages,
    setMessages,
    setStreaming: noop,
    setActivityHint: noop,
    setError: noop,
  });
  const [size, setSize] = useState(1000);
  const [width, setWidth] = useState(420);
  const [kind, setKind] = useState("mixed");
  const [autoShowAll, setAutoShowAll] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [summary, setSummary] = useState("");
  const metrics = useRef(blankMetrics());
  const host = useRef<HTMLDivElement>(null);
  const source = useRef("");
  const started = useRef(0);
  const sequence = useRef(0);
  const runRef = useRef("");
  const stoppedRef = useRef(false);
  const anchorRef = useRef<{ node: Element; offset: number } | null>(null);
  function testUpwardScroll() {
    const viewport = host.current?.querySelector<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    if (!viewport) return;
    viewport.dispatchEvent(
      new WheelEvent("wheel", { deltaY: -10, bubbles: true }),
    );
    viewport.scrollTop = Math.max(0, viewport.scrollTop - 100);
    requestAnimationFrame(() => {
      const top = viewport.getBoundingClientRect().top;
      const node = [...viewport.querySelectorAll("[data-source-row]")].find(
        (item) => item.getBoundingClientRect().bottom > top + 12,
      );
      anchorRef.current = node
        ? { node, offset: node.getBoundingClientRect().top - top }
        : null;
    });
  }
  function start() {
    const run = `synthetic-${++sequence.current}`;
    runRef.current = run;
    stoppedRef.current = false;
    const seed =
      kind === "code"
        ? "synthetic_code_line();\n"
        : kind === "paragraph"
          ? "连续的合成文字与英文 word "
          : paragraphs;
    const body = seed.repeat(Math.ceil(size / seed.length)).slice(0, size);
    source.current =
      kind === "code" ? `\x60\x60\x60ts\n${body}\n\x60\x60\x60` : body;
    metrics.current = blankMetrics();
    anchorRef.current = null;
    setSummary("");
    started.current = performance.now();
    setMessages((previous) => [
      ...previous.map((message) =>
        message.answerPresentation
          ? {
              ...message,
              answerPresentation: {
                ...message.answerPresentation,
                settled: true,
              },
            }
          : message,
      ),
      { role: "user", content: `合成回放 ${size} 字符`, clientRequestId: run },
    ]);
    setPlaying(true);
  }
  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    let clockStarted = false;
    let previousBody: Element | null = null;
    let completeAt = 0;
    let presentedAt = 0;
    let lastPublication = 0;
    let firstVisibleAt = 0;
    let draftFrames = 0;
    let clearedFrames = 0;
    let prematureCompleteFrames = 0;
    let wasVisible = false;
    let confirmedTarget = "";
    let targetReplacements = 0;
    const scrolls: { t: number; top: number; max: number }[] = [];
    let upwardWheels = 0;
    const observedViewport = host.current?.querySelector<HTMLElement>(
      "[data-radix-scroll-area-viewport]",
    );
    const recordScroll = () => {
      if (observedViewport)
        scrolls.push({
          t: performance.now() - started.current,
          top: observedViewport.scrollTop,
          max: observedViewport.scrollHeight - observedViewport.clientHeight,
        });
    };
    const recordWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) upwardWheels += 1;
    };
    observedViewport?.addEventListener("scroll", recordScroll);
    observedViewport?.addEventListener("wheel", recordWheel);
    const longTaskObserver =
      typeof PerformanceObserver === "undefined"
        ? null
        : new PerformanceObserver((list) => {
            metrics.current.longTasks.push(
              ...list.getEntries().map((entry) => entry.duration),
            );
          });
    if (PerformanceObserver.supportedEntryTypes?.includes("longtask"))
      longTaskObserver?.observe({ type: "longtask" });
    const tick = (now: number) => {
      if (!clockStarted) {
        started.current = now;
        clockStarted = true;
      }
      const elapsed = now - started.current;
      const run = runRef.current;
      // Intake binds after 150ms; target arrives in bursts over 2s, then completes.
      const end = Math.min(
        source.current.length,
        Math.floor((Math.max(0, elapsed - 150) / 2000) * source.current.length),
      );
      if (
        elapsed >= 150 &&
        now - lastPublication >= 80 &&
        (!completeAt || now - completeAt < 800) &&
        !stoppedRef.current
      ) {
        lastPublication = now;
        const complete = end === source.current.length;
        setMessages((previous) => {
          const next = previous.map((message) =>
            message.clientRequestId === run && message.role === "user"
              ? { ...message, runId: run, turnId: `turn-${run}` }
              : message,
          );
          return next;
        });
        // Replay both historical withdrawal triggers through the production projection:
        // private candidate -> tools/reset, then committed body -> late reset/short snapshot.
        const epoch =
          completeAt && now - completeAt > 350 ? 2 : elapsed > 850 ? 1 : 0;
        setRun({
          ...createAssistantRunEventState(run),
          state: complete ? "completed" : "running",
          lastSeq: Math.floor(elapsed / 80),
          content: complete
            ? epoch === 2
              ? "较短的迟到版本"
              : source.current
            : "",
          stage: complete ? null : "正在检索并整理答复",
        });
        setPresentation({
          ...createAssistantPresentationState(run),
          lastSeq: Math.floor(elapsed / 80),
          resetEpoch: epoch,
          answer: epoch ? "" : "不应出现在正文中的候选文字",
          answerComplete: complete,
          processItems: complete
            ? []
            : [
                {
                  id: "synthetic-tool",
                  kind: "tool",
                  label: "联网搜索",
                  status: "running",
                  elapsedMs: 150,
                },
              ],
        });
        if (complete && !completeAt) completeAt = now;
      }
      const viewport = host.current?.querySelector<HTMLElement>(
        "[data-radix-scroll-area-viewport]",
      );
      const answers = host.current?.querySelectorAll('[data-role="assistant"]');
      const answer = answers?.[answers.length - 1];
      const body =
        answer?.querySelector("[data-ai-streaming-markdown]") ?? null;
      if (previousBody && body && previousBody !== body)
        metrics.current.bodyReplacements += 1;
      if (body) previousBody = body;
      const projected = messagesRef.current.find(
        (message) => message.role === "assistant" && message.runId === run,
      );
      if (projected?.answerPresentation?.complete) {
        if (confirmedTarget && projected.content !== confirmedTarget)
          targetReplacements += 1;
        confirmedTarget = projected.content;
      }
      const visible = Boolean(body?.textContent?.trim());
      if (visible && !firstVisibleAt) firstVisibleAt = elapsed;
      if (visible && !completeAt) draftFrames += 1;
      if (wasVisible && !visible) clearedFrames += 1;
      wasVisible ||= visible;
      const phase =
        answer?.getAttribute("data-presentation-phase") ?? "pending";
      if (phase !== "complete" && answer?.textContent?.includes("答复完毕"))
        prematureCompleteFrames += 1;
      metrics.current.samples.push({
        t: Math.round(elapsed * 100) / 100,
        top: viewport?.scrollTop ?? 0,
        height: viewport?.scrollHeight ?? 0,
        blocks:
          host.current?.querySelectorAll("[data-markdown-block]").length ?? 0,
        phase,
        observers: observerStats.instances,
        observedNodes: observerStats.observed,
        following: !host.current?.querySelector('[aria-label="回到最新"]'),
      });
      if (autoShowAll && completeAt && phase === "draining")
        answer
          ?.querySelector<HTMLButtonElement>('[aria-label="立即显示全部"]')
          ?.click();
      const anchor = anchorRef.current;
      if (anchor?.node.isConnected && viewport)
        metrics.current.maxAnchorDrift = Math.max(
          metrics.current.maxAnchorDrift,
          Math.abs(
            anchor.node.getBoundingClientRect().top -
              viewport.getBoundingClientRect().top -
              anchor.offset,
          ),
        );
      metrics.current.maxNodes = Math.max(
        metrics.current.maxNodes,
        host.current?.querySelectorAll("*").length ?? 0,
      );
      if ((completeAt && phase === "complete") || phase === "stopped")
        presentedAt ||= now;
      else presentedAt = 0;
      // Include final layout/observer/scroll work in the recording, not just
      // the React commit which first reports presentation completion.
      if (presentedAt && now - presentedAt >= 1000) {
        const data = metrics.current;
        const detached = data.samples.findIndex((sample) => !sample.following);
        const detachTime = data.samples[detached]?.t ?? -1;
        const sorted = [...data.renderMs].sort((a, b) => a - b);
        const report = JSON.stringify(
          {
            frames: data.samples.length,
            draftFrames,
            targetReplacements,
            finalTargetChars: confirmedTarget.length,
            clearedFrames,
            prematureCompleteFrames,
            researchMs: completeAt ? completeAt - started.current : null,
            firstVisibleMs: firstVisibleAt || null,
            playbackCompleteMs: presentedAt
              ? presentedAt - started.current
              : null,
            renderP95Ms: sorted[Math.floor(sorted.length * 0.95)] ?? 0,
            longTasks: data.longTasks.length,
            longTaskSupported:
              PerformanceObserver.supportedEntryTypes?.includes("longtask") ??
              false,
            maxLongTaskMs: Math.max(0, ...data.longTasks),
            bodyReplacements: data.bodyReplacements,
            maxNodes: data.maxNodes,
            maxAnchorDrift: data.maxAnchorDrift,
            observers: observerStats.instances,
            observedNodes: observerStats.observed,
            phase,
            viewportHeight: viewport?.clientHeight ?? 0,
            finalScrollTop: viewport?.scrollTop ?? 0,
            finalScrollHeight: viewport?.scrollHeight ?? 0,
            firstDetachedAt:
              data.samples.find((sample) => !sample.following)?.t ?? null,
            upwardWheels,
            detachFrames:
              detached < 0
                ? []
                : data.samples.slice(Math.max(0, detached - 4), detached + 4),
            detachScrolls:
              detached < 0
                ? []
                : scrolls.filter(
                    (sample) => Math.abs(sample.t - detachTime) < 100,
                  ),
            durationMs: Math.round(elapsed),
          },
          null,
          2,
        );
        setSummary(report);
        // Optional loopback collector for native windows without a WebDriver bridge.
        if (new URLSearchParams(location.search).get("report") === "1") {
          void fetch("http://127.0.0.1:17543/metrics", {
            method: "POST",
            body: report,
          }).catch(() => undefined);
        }
        setPlaying(false);
      } else frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      longTaskObserver?.disconnect();
      observedViewport?.removeEventListener("scroll", recordScroll);
      observedViewport?.removeEventListener("wheel", recordWheel);
    };
  }, [playing, autoShowAll]);
  function download() {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(metrics.current)], { type: "application/json" }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = "iris-synthetic-stream-metrics.json";
    link.click();
    URL.revokeObjectURL(url);
  }
  return (
    <main
      style={{ height: "100vh", overflow: "hidden" }}
      className="flex flex-col gap-3 bg-background p-4 text-foreground"
    >
      <div className="flex flex-wrap items-center gap-3">
        <label>
          长度{" "}
          <select
            aria-label="回放长度"
            value={size}
            onChange={(event) => setSize(Number(event.target.value))}
          >
            {[1000, 8000, 32000, 80000, 200000].map((value) => (
              <option key={value}>{value}</option>
            ))}
          </select>
        </label>
        <label>
          类型{" "}
          <select
            aria-label="回放类型"
            value={kind}
            onChange={(event) => setKind(event.target.value)}
          >
            <option value="mixed">混合 Markdown</option>
            <option value="paragraph">超长段落</option>
            <option value="code">代码</option>
          </select>
        </label>
        <label>
          宽度{" "}
          <input
            aria-label="面板宽度"
            type="range"
            min={280}
            max={1000}
            value={width}
            onChange={(event) => setWidth(Number(event.target.value))}
          />
        </label>
        <button
          data-testid="stream-replay-start"
          onClick={start}
          disabled={playing}
        >
          开始回放
        </button>
        <button
          onClick={() => {
            stoppedRef.current = true;
            setMessages((previous) =>
              previous.map((message) =>
                message.answerPresentation
                  ? {
                      ...message,
                      answerPresentation: {
                        ...message.answerPresentation,
                        stopped: true,
                      },
                      presentationStreaming: false,
                    }
                  : message,
              ),
            );
          }}
        >
          取消显示
        </button>
        <button onClick={testUpwardScroll}>测试上滚</button>
        <label>
          <input
            type="checkbox"
            checked={autoShowAll}
            onChange={(event) => setAutoShowAll(event.target.checked)}
          />
          到达后自动显示全部
        </label>
        <button
          onClick={() => {
            const buttons = host.current?.querySelectorAll<HTMLButtonElement>(
              '[aria-label="立即显示全部"]',
            );
            buttons?.[buttons.length - 1]?.click();
          }}
        >
          测试显示全部
        </button>
        <button onClick={download}>导出数值记录</button>
      </div>
      <div className="flex min-h-0 flex-1 gap-4">
        <div
          ref={host}
          style={{ width, maxWidth: "100%" }}
          className="ai-sidecar flex min-h-0 flex-col border border-border"
          data-ai-domain="normal"
        >
          <Profiler
            id="conversation"
            onRender={(_id, _phase, duration) =>
              metrics.current.renderMs.push(duration)
            }
          >
            <AiMessageList
              messages={messages}
              streaming={
                playing && !messages.at(-1)?.answerPresentation?.complete
              }
            />
          </Profiler>
        </div>
        <pre data-testid="stream-replay-metrics" className="text-xs">
          {summary ||
            (playing
              ? "回放中：可上滚、调整宽度、点击立即显示全部。"
              : "仅合成输入，无模型请求。")}
        </pre>
      </div>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<App />);

// Opt-in automation for native WebKit/WebView2; still synthetic and IPC-free.
if (new URLSearchParams(window.location.search).has("auto")) {
  const begin = () => {
    const button = document.querySelector<HTMLButtonElement>(
      '[data-testid="stream-replay-start"]',
    );
    if (button) button.click();
    else window.setTimeout(begin, 100);
  };
  window.setTimeout(begin, 100);
}
