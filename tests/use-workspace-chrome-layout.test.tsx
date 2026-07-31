import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useWorkspaceChromeLayout } from "@/hooks/useWorkspaceChromeLayout";
import type { UseWorkspaceChromeLayoutOptions } from "@/hooks/useWorkspaceChromeLayout";

type HookApi = ReturnType<typeof useWorkspaceChromeLayout>;

class FakeResizeObserver {
  static instances: FakeResizeObserver[] = [];
  callback: ResizeObserverCallback;
  targets = new Set<Element>();

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    FakeResizeObserver.instances.push(this);
  }

  observe(target: Element): void {
    this.targets.add(target);
  }

  unobserve(target: Element): void {
    this.targets.delete(target);
  }

  disconnect(): void {
    this.targets.clear();
  }

  fire(widthPx: number): void {
    const entries = [...this.targets].map((target) => ({
      target,
      contentRect: {
        width: widthPx,
        height: 0,
        x: 0,
        y: 0,
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
      },
    }));
    this.callback(
      entries as unknown as ResizeObserverEntry[],
      this as unknown as ResizeObserver,
    );
  }
}

function Harness({
  apiRef,
  options,
}: {
  apiRef: { current: HookApi | null };
  options?: UseWorkspaceChromeLayoutOptions;
}) {
  const api = useWorkspaceChromeLayout(options);
  apiRef.current = api;
  return createElement("div", {
    ref: api.containerRef,
    "data-testid": "content",
  });
}

/** 根字号 16px：52rem=832、18rem=288、25rem=400、30rem=480、45rem=720。 */
const DOC = 832;
const NAV = 288;
const AGENT_MIN = 400;
const AGENT_TARGET = 480;
const AGENT_MAX = 720;

describe("useWorkspaceChromeLayout", () => {
  let host: HTMLDivElement;
  let root: Root;
  let apiRef: { current: HookApi | null };
  let computedStyleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    FakeResizeObserver.instances = [];
    apiRef = { current: null };
    computedStyleSpy = vi.spyOn(window, "getComputedStyle").mockReturnValue({
      fontSize: "16px",
      getPropertyValue: (prop: string) =>
        prop === "--prose-measure" ? "52rem" : "",
    } as CSSStyleDeclaration);
    localStorage.clear();

    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    globalThis.ResizeObserver = FakeResizeObserver;
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    computedStyleSpy.mockRestore();
    delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
    localStorage.clear();
  });

  function renderHook(options?: UseWorkspaceChromeLayoutOptions) {
    act(() => {
      root.render(createElement(Harness, { apiRef, options }));
    });
  }

  function fireResize(widthPx: number) {
    act(() => {
      FakeResizeObserver.instances[0]?.fire(widthPx);
    });
  }

  it("初始投影：默认写作态（导航关闭 + Agent 侧车），宽度来自 ResizeObserver", () => {
    renderHook();
    fireResize(2000);

    const api = apiRef.current;
    expect(api).not.toBeNull();
    expect(api?.contentWidthPx).toBe(2000);
    expect(api?.rootFontSizePx).toBe(16);
    expect(api?.proseMeasurePx).toBe(DOC);
    expect(api?.budgets).toEqual({
      documentProtectedPx: DOC,
      navigatorPinnedPx: NAV,
      agentMinPx: AGENT_MIN,
      agentTargetPx: AGENT_TARGET,
      agentMaxPx: AGENT_MAX,
    });
    expect(api?.projection).toMatchObject({
      primarySurface: "document",
      navigator: "closed",
      assistant: "sidecar",
      sidecarWidthPx: AGENT_TARGET,
    });
  });

  it("打开导航并固定：宽度充足时 pinned；resize 缩窄退回 peek 且意图不变", () => {
    renderHook();
    fireResize(2000);

    act(() => {
      apiRef.current?.setNavigatorOpen(true);
      apiRef.current?.setPinPreferred(true);
    });
    expect(apiRef.current?.projection.navigator).toBe("pinned");
    expect(apiRef.current?.projection.pinnedEligible).toBe(true);

    fireResize(1500); // 1600 不够，导航退回 peek，Agent 保持
    expect(apiRef.current?.projection.navigator).toBe("peek");
    expect(apiRef.current?.projection.assistant).toBe("sidecar");
    expect(apiRef.current?.navigatorOpen).toBe(true);
    expect(apiRef.current?.pinPreferred).toBe(true);

    fireResize(1200); // 侧车折叠，但意图不变
    expect(apiRef.current?.projection.assistant).toBe("collapsed");
    expect(apiRef.current?.aiPanelOpen).toBe(true);
    expect(apiRef.current?.primarySurface).toBe("document");

    fireResize(2000); // 恢复宽度后不自动打开已关闭导航
    expect(apiRef.current?.projection.navigator).toBe("pinned");
  });

  it("resize 不自动进入 focus；focus 意图也不因 resize 自动退出", () => {
    renderHook();
    fireResize(800);

    expect(apiRef.current?.projection.assistant).toBe("collapsed");
    expect(apiRef.current?.projection.assistant).not.toBe("focus");

    act(() => {
      apiRef.current?.enterAssistantFocus();
    });
    expect(apiRef.current?.projection.assistant).toBe("focus");

    fireResize(2000);
    fireResize(600);
    expect(apiRef.current?.projection.assistant).toBe("focus");
    expect(apiRef.current?.primarySurface).toBe("assistant_focus");

    act(() => {
      apiRef.current?.exitAssistantFocus();
    });
    expect(apiRef.current?.projection.assistant).toBe("collapsed");
  });

  it("openAssistant：宽度足够时打开侧车，不足时进入 focus", () => {
    renderHook();
    fireResize(800);

    act(() => {
      apiRef.current?.openAssistant();
    });
    expect(apiRef.current?.projection.assistant).toBe("focus");

    act(() => {
      apiRef.current?.exitAssistantFocus();
    });
    fireResize(2000);
    act(() => {
      apiRef.current?.openAssistant();
    });
    expect(apiRef.current?.projection.assistant).toBe("sidecar");
  });

  it("只持久化允许项：固定偏好写入 localStorage，导航打开状态不持久化", () => {
    renderHook();
    fireResize(2000);

    act(() => {
      apiRef.current?.setPinPreferred(true);
      apiRef.current?.setNavigatorOpen(true);
    });

    expect(
      localStorage.getItem("iris.workspaceChrome.navigatorPinPreferred"),
    ).toBe("1");
    expect(
      localStorage.getItem("iris.workspaceChrome.navigatorOpen"),
    ).toBeNull();

    // 重新挂载后固定偏好恢复，导航保持关闭
    act(() => root.unmount());
    host.remove();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    renderHook();
    fireResize(2000);
    expect(apiRef.current?.pinPreferred).toBe(true);
    expect(apiRef.current?.navigatorOpen).toBe(false);
  });

  it("setSidecarWidth：按 rem 预算 clamp 并持久化", () => {
    renderHook();
    fireResize(2000);

    act(() => {
      apiRef.current?.setSidecarWidth(2000);
    });
    expect(apiRef.current?.savedSidecarWidthPx).toBe(AGENT_MAX);
    expect(localStorage.getItem("iris.aiPanelWidth")).toBe(String(AGENT_MAX));

    act(() => {
      apiRef.current?.setSidecarWidth(100);
    });
    expect(apiRef.current?.savedSidecarWidthPx).toBe(AGENT_MIN);
    expect(localStorage.getItem("iris.aiPanelWidth")).toBe(String(AGENT_MIN));
    expect(apiRef.current?.projection.sidecarWidthPx).toBe(AGENT_MIN);
  });

  it("根字号变化后预算重新计算，投影随之更新", () => {
    renderHook();
    fireResize(2000);

    computedStyleSpy.mockReturnValue({
      fontSize: "20px",
      getPropertyValue: (prop: string) =>
        prop === "--prose-measure" ? "52rem" : "",
    } as CSSStyleDeclaration);
    fireResize(2000);

    expect(apiRef.current?.rootFontSizePx).toBe(20);
    expect(apiRef.current?.proseMeasurePx).toBe(1040);
    expect(apiRef.current?.budgets.agentMinPx).toBe(500);
    expect(apiRef.current?.budgets.agentTargetPx).toBe(600);

    act(() => {
      apiRef.current?.setSidecarWidth(2000);
    });
    expect(apiRef.current?.savedSidecarWidthPx).toBe(900); // 45rem @ 20px
  });

  it("禅模式覆盖有效 presentation，不覆盖用户意图", () => {
    renderHook({ zenMode: true });
    fireResize(2000);

    act(() => {
      apiRef.current?.setNavigatorOpen(true);
      apiRef.current?.setPinPreferred(true);
      apiRef.current?.enterAssistantFocus();
    });
    expect(apiRef.current?.projection.navigator).toBe("closed");
    expect(apiRef.current?.projection.assistant).toBe("collapsed");
    expect(apiRef.current?.primarySurface).toBe("assistant_focus");

    act(() => {
      apiRef.current?.exitAssistantFocus();
    });
    expect(apiRef.current?.projection.primarySurface).toBe("document");
  });
});
