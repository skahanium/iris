import { readFileSync } from "node:fs";
import { act, useEffect } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppShell } from "@/components/layout/AppShell";
import type { WorkspacePrimarySurface } from "@/lib/workspace-chrome-layout";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

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

const mountCounts: Record<string, number> = {};

function Sentinel({ label }: { label: string }) {
  useEffect(() => {
    mountCounts[label] = (mountCounts[label] ?? 0) + 1;
  }, [label]);
  return <div data-testid={`sentinel-${label}`} />;
}

interface ShellProps {
  aiPanelOpen?: boolean;
  navigatorOpen?: boolean;
  pinPreferred?: boolean;
  primarySurface?: WorkspacePrimarySurface;
  zen?: boolean;
}

function renderShell(props: ShellProps = {}) {
  return render(
    <AppShell
      tabBar={<Sentinel label="tab" />}
      editor={<Sentinel label="editor" />}
      aiPanel={<Sentinel label="agent" />}
      navigator={<Sentinel label="navigator" />}
      statusBar={<Sentinel label="status" />}
      aiPanelOpen={props.aiPanelOpen}
      navigatorOpen={props.navigatorOpen}
      pinPreferred={props.pinPreferred}
      primarySurface={props.primarySurface}
      zen={props.zen}
    />,
  );
}

function fireResize(widthPx: number) {
  act(() => {
    FakeResizeObserver.instances[FakeResizeObserver.instances.length - 1]?.fire(
      widthPx,
    );
  });
}

function fireAnimationEnd(element: Element, animationName: string) {
  const event = new Event("animationend", { bubbles: true });
  Object.defineProperty(event, "animationName", {
    configurable: true,
    value: animationName,
  });
  fireEvent(element, event);
}

describe("AppShell 自适应布局：单实例稳定挂载与投影", () => {
  let computedStyleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    FakeResizeObserver.instances = [];
    Object.keys(mountCounts).forEach((key) => delete mountCounts[key]);
    computedStyleSpy = vi.spyOn(window, "getComputedStyle").mockReturnValue({
      fontSize: "16px",
      getPropertyValue: (prop: string) =>
        prop === "--prose-measure" ? "52rem" : "",
    } as CSSStyleDeclaration);
    // jsdom 未实现 pointer capture，拖拽测试 stub 掉该浏览器 API。
    Object.defineProperty(Element.prototype, "setPointerCapture", {
      configurable: true,
      value: vi.fn(),
    });
    // jsdom 缺少可构造的 PointerEvent，RTL 会回退到 Event 丢失 clientX；
    // 提供最小 polyfill 以便拖拽测试能读到指针坐标。
    class TestPointerEvent extends MouseEvent {
      pointerId: number;
      constructor(type: string, init: PointerEventInit = {}) {
        super(type, init);
        this.pointerId = init.pointerId ?? 0;
      }
    }
    Object.defineProperty(globalThis, "PointerEvent", {
      configurable: true,
      value: TestPointerEvent,
    });
    localStorage.clear();
    globalThis.ResizeObserver = FakeResizeObserver;
  });

  afterEach(() => {
    cleanup();
    computedStyleSpy.mockRestore();
    delete (Element.prototype as { setPointerCapture?: unknown })
      .setPointerCapture;
    delete (globalThis as { PointerEvent?: unknown }).PointerEvent;
    delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
    localStorage.clear();
  });

  it("sidecar ↔ focus 往返不 remount Agent 与 editor，editor 保持不可交互", () => {
    const view = renderShell({ aiPanelOpen: true });
    fireResize(2000);

    expect(
      screen
        .getByTestId("unified-assistant-dock")
        .getAttribute("data-presentation"),
    ).toBe("sidecar");
    const agentMounts = mountCounts.agent ?? 0;
    const editorMounts = mountCounts.editor ?? 0;
    expect(agentMounts).toBe(1);
    expect(editorMounts).toBe(1);
    expect(
      screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
    ).toBeNull();

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        primarySurface="assistant_focus"
      />,
    );
    fireResize(2000);

    expect(
      screen
        .getByTestId("unified-assistant-dock")
        .getAttribute("data-presentation"),
    ).toBe("focus");
    expect(mountCounts.agent).toBe(agentMounts);
    expect(mountCounts.editor).toBe(editorMounts);
    const main = screen.getByTestId("workspace-main");
    expect(main.getAttribute("aria-hidden")).toBe("true");
    expect(main.className).toContain("pointer-events-none");

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        primarySurface="document"
      />,
    );
    fireResize(2000);

    expect(mountCounts.agent).toBe(agentMounts);
    expect(mountCounts.editor).toBe(editorMounts);
    expect(
      screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
    ).toBeNull();
  });

  it("peek ↔ pinned 往返不 remount 导航子树", () => {
    renderShell({
      navigatorOpen: true,
      pinPreferred: true,
    });
    fireResize(2000);

    const navigatorNode = screen.getByTestId("workspace-navigator");
    expect(navigatorNode.getAttribute("data-presentation")).toBe("pinned");
    const navigatorMounts = mountCounts.navigator ?? 0;
    expect(navigatorMounts).toBe(1);

    fireResize(1500);
    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("peek");
    expect(mountCounts.navigator).toBe(navigatorMounts);

    fireResize(2000);
    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("pinned");
    expect(mountCounts.navigator).toBe(navigatorMounts);
  });

  it("keeps the navigator a block container while constraining its viewport", () => {
    renderShell({ navigatorOpen: true });
    fireResize(2000);

    const navigatorNode = screen.getByTestId("workspace-navigator");
    expect(navigatorNode.className).toContain("h-full");
    expect(navigatorNode.className).toContain("min-h-0");
    expect(navigatorNode.className).toContain("overflow-hidden");
    expect(navigatorNode.className).not.toContain("flex");
  });

  it("peek 悬浮呈现使用高于目录岛的 z 层（z-navigator-overlay），pinned 保持 z-navigator", () => {
    renderShell({
      navigatorOpen: true,
      pinPreferred: true,
    });
    fireResize(2000);

    const pinned = screen.getByTestId("workspace-navigator");
    expect(pinned.getAttribute("data-presentation")).toBe("pinned");
    // pinned 是独立 flex 列，与目录岛无几何重叠，保持普通层即可。
    expect(pinned.className).toContain("z-navigator");
    expect(pinned.className).not.toContain("z-navigator-overlay");

    fireResize(1500);
    const peek = screen.getByTestId("workspace-navigator");
    expect(peek.getAttribute("data-presentation")).toBe("peek");
    // peek 是覆盖在编辑器之上的悬浮层：必须高于目录岛（z-editor-chrome: 15），
    // 否则目录岛会绘制在文件树之上（目录岛悬浮到文件树模块上的回归）。
    expect(peek.className).toContain("z-navigator-overlay");
    // 只允许 overlay token，不允许与普通 z-navigator 共存（tailwind-merge 会去重，
    // 此断言保护类书写不回归成两个 z token 并存）。
    expect(peek.className).not.toMatch(/(^|\s)z-navigator(\s|$)/);
  });

  it("z 序契约：navigator-overlay(18) 介于 editor-chrome(15) 与 workspace-focus(20) 之间", () => {
    const config = read("tailwind.config.js");
    const block = config.match(/zIndex:\s*\{[\s\S]*?\n\s*\}/);
    expect(block).not.toBeNull();
    const zIndex = block![0];
    expect(zIndex).toContain('"editor-chrome": "15"');
    expect(zIndex).toContain('"navigator-overlay": "18"');
    expect(zIndex).toContain('"workspace-focus": "20"');
  });

  it("导航 closed 时不渲染导航子树；peek 在 focus 中隐藏但仍挂载", () => {
    renderShell({});
    fireResize(2000);
    expect(screen.queryByTestId("workspace-navigator")).toBeNull();

    const view = renderShell({ navigatorOpen: true });
    fireResize(2000);
    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("peek");
    const navigatorMounts = mountCounts.navigator ?? 0;

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        navigatorOpen
        primarySurface="assistant_focus"
      />,
    );
    fireResize(2000);

    expect(mountCounts.navigator).toBe(navigatorMounts);
    expect(
      screen.getByTestId("workspace-navigator").getAttribute("aria-hidden"),
    ).toBe("true");
    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("peek");
  });

  it("导航关闭时淡出后卸载，关闭中重新打开不受旧动画影响", () => {
    const view = renderShell({ navigatorOpen: true });
    fireResize(2000);

    const renderWithNavigatorOpen = (navigatorOpen: boolean) => {
      view.rerender(
        <AppShell
          tabBar={<Sentinel label="tab" />}
          editor={<Sentinel label="editor" />}
          aiPanel={<Sentinel label="agent" />}
          navigator={<Sentinel label="navigator" />}
          statusBar={<Sentinel label="status" />}
          navigatorOpen={navigatorOpen}
        />,
      );
      fireResize(2000);
    };

    renderWithNavigatorOpen(false);

    const closingNavigator = screen.getByTestId("workspace-navigator");
    expect(closingNavigator.getAttribute("data-transition")).toBe("exiting");
    expect(closingNavigator.getAttribute("aria-hidden")).toBe("true");
    expect(closingNavigator.className).toContain("pointer-events-none");
    expect(closingNavigator.className).toContain(
      "motion-safe:animate-iris-fade-out",
    );
    expect(closingNavigator.style.animationFillMode).toBe("both");
    fireAnimationEnd(closingNavigator, "iris-fade-in");
    expect(screen.getByTestId("workspace-navigator")).toBe(closingNavigator);

    renderWithNavigatorOpen(true);

    const reopenedNavigator = screen.getByTestId("workspace-navigator");
    expect(reopenedNavigator.getAttribute("data-transition")).toBe("entering");
    fireEvent.animationEnd(reopenedNavigator);
    expect(screen.getByTestId("workspace-navigator")).toBe(reopenedNavigator);

    renderWithNavigatorOpen(false);
    const finalClosingNavigator = screen.getByTestId("workspace-navigator");
    fireAnimationEnd(finalClosingNavigator, "iris-fade-out");
    expect(screen.queryByTestId("workspace-navigator")).toBeNull();
  });

  it("减弱动效时导航关闭立即卸载", () => {
    const originalMatchMedia = window.matchMedia;
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn().mockReturnValue({ matches: true }),
    });

    try {
      const view = renderShell({ navigatorOpen: true });
      fireResize(2000);
      expect(screen.getByTestId("workspace-navigator")).toBeTruthy();

      view.rerender(
        <AppShell
          tabBar={<Sentinel label="tab" />}
          editor={<Sentinel label="editor" />}
          aiPanel={<Sentinel label="agent" />}
          navigator={<Sentinel label="navigator" />}
          statusBar={<Sentinel label="status" />}
          navigatorOpen={false}
        />,
      );
      fireResize(2000);

      expect(screen.queryByTestId("workspace-navigator")).toBeNull();
    } finally {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: originalMatchMedia,
      });
    }
  });

  it("focus 时面板与内容容器不残留侧车像素宽度，占满主工作区", () => {
    const view = renderShell({ aiPanelOpen: true });
    fireResize(2000);
    const dock = screen.getByTestId("unified-assistant-dock");
    expect(dock.getAttribute("data-presentation")).toBe("sidecar");
    // 侧车：aside 持有侧车像素宽度。
    expect(dock.style.width).toBe("480px");

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        primarySurface="assistant_focus"
      />,
    );
    fireResize(2000);

    expect(dock.getAttribute("data-presentation")).toBe("focus");
    // 回归防护：focus 时 aside 不能被内联 width:0 压制，内容容器不得残留 savedSidecarWidthPx，
    // 否则面板停留在侧车宽度、右侧大片空白，--ai-focus-measure 内容列永远达不到。
    expect(dock.style.width).toBe("");
    const wrapper = dock.firstElementChild as HTMLElement;
    expect(wrapper.style.width).toBe("");
  });

  it("Agent collapsed 时侧车保持挂载并 aria-hidden", () => {
    renderShell({ aiPanelOpen: true });
    fireResize(2000);
    const agentMounts = mountCounts.agent ?? 0;

    fireResize(800);
    const dock = screen.getByTestId("unified-assistant-dock");
    expect(dock.getAttribute("data-presentation")).toBe("collapsed");
    expect(dock.getAttribute("aria-hidden")).toBe("true");
    expect(mountCounts.agent).toBe(agentMounts);

    fireResize(2000);
    expect(
      screen.getByTestId("unified-assistant-dock").getAttribute("aria-hidden"),
    ).toBeNull();
    expect(mountCounts.agent).toBe(agentMounts);
  });

  it("禅模式隐藏导航与 Agent 但不卸载；退出后恢复 presentation", () => {
    const view = renderShell({
      aiPanelOpen: true,
      navigatorOpen: true,
      pinPreferred: true,
    });
    fireResize(2000);
    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("pinned");
    const agentMounts = mountCounts.agent ?? 0;

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        navigatorOpen
        pinPreferred
        zen
      />,
    );
    fireResize(2000);

    const dock = screen.getByTestId("unified-assistant-dock");
    expect(dock.getAttribute("data-presentation")).toBe("collapsed");
    expect(dock.getAttribute("aria-hidden")).toBe("true");
    expect(mountCounts.agent).toBe(agentMounts);
    expect(screen.queryByTestId("workspace-navigator")).toBeNull();

    view.rerender(
      <AppShell
        tabBar={<Sentinel label="tab" />}
        editor={<Sentinel label="editor" />}
        aiPanel={<Sentinel label="agent" />}
        navigator={<Sentinel label="navigator" />}
        statusBar={<Sentinel label="status" />}
        aiPanelOpen
        navigatorOpen
        pinPreferred
      />,
    );
    fireResize(2000);

    expect(
      screen
        .getByTestId("workspace-navigator")
        .getAttribute("data-presentation"),
    ).toBe("pinned");
    expect(
      screen
        .getByTestId("unified-assistant-dock")
        .getAttribute("data-presentation"),
    ).toBe("sidecar");
    expect(mountCounts.agent).toBe(agentMounts);
  });

  it("侧车拖拽调整宽度按预算 clamp 并持久化", () => {
    renderShell({ aiPanelOpen: true });
    fireResize(2000);
    const dock = screen.getByTestId("unified-assistant-dock");
    expect(dock.getAttribute("data-presentation")).toBe("sidecar");

    const separator = screen.getByRole("separator", {
      name: "调整 AI 侧栏宽度",
    });
    fireEvent.pointerDown(separator, { pointerId: 1, clientX: 800 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 1500 });
    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(localStorage.getItem("iris.aiPanelWidth")).not.toBeNull();
    expect(
      Number(localStorage.getItem("iris.aiPanelWidth")),
    ).toBeGreaterThanOrEqual(400);
    expect(
      Number(localStorage.getItem("iris.aiPanelWidth")),
    ).toBeLessThanOrEqual(720);
  });
});
