import type { MutableRefObject, ReactNode, RefObject } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantComposerDock } from "@/components/ai/AssistantComposerDock";
import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import { ConversationSurface } from "@/components/ai/ConversationSurface";
import { AppShell } from "@/components/layout/AppShell";
import { useWorkspaceChromeActions } from "@/hooks/useWorkspaceChromeActions";
import type { PromptProfileDto } from "@/lib/ipc";
import { act } from "react";

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

function fireResize(widthPx: number) {
  act(() => {
    FakeResizeObserver.instances[FakeResizeObserver.instances.length - 1]?.fire(
      widthPx,
    );
  });
}

function shellProps(
  aiPanel: ReactNode,
  props: { aiPanelOpen?: boolean; zen?: boolean } = {},
) {
  return (
    <AppShell
      tabBar={<div data-testid="shell-tabbar">tabs</div>}
      editor={<div data-testid="shell-editor">editor</div>}
      aiPanel={aiPanel}
      statusBar={<div data-testid="shell-statusbar">status</div>}
      aiPanelOpen={props.aiPanelOpen}
      zen={props.zen}
    />
  );
}

/** 模拟 Agent 面板内的布局动作按钮（经 AppShell 的 WorkspaceChromeActions 通道）。 */
function FocusActionProbe() {
  const actions = useWorkspaceChromeActions();
  return (
    <div data-testid="probe-inside">
      <button
        type="button"
        data-testid="probe-enter"
        onClick={actions.enterAssistantFocus}
      >
        展开阅读
      </button>
      <button
        type="button"
        data-testid="probe-exit"
        onClick={actions.exitAssistantFocus}
      >
        返回文档
      </button>
      <span data-testid="probe-surface">
        {actions.projection.primarySurface}
      </span>
    </div>
  );
}

const PROFILE: PromptProfileDto = {
  display_name: "测试助手",
  avatar_id: "iris",
  persona: "test",
  writing_style: "plain",
  custom_rules: [],
  behavior: {
    initiative: "balanced",
    directness: "balanced",
    tone: "natural",
    challenge: "balanced",
  },
  language: "zh",
};

describe("Agent 主区阅读（assistant focus surface）", () => {
  let computedStyleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    FakeResizeObserver.instances = [];
    computedStyleSpy = vi.spyOn(window, "getComputedStyle").mockReturnValue({
      fontSize: "16px",
      getPropertyValue: (prop: string) =>
        prop === "--prose-measure" ? "52rem" : "",
    } as CSSStyleDeclaration);
    Object.defineProperty(Element.prototype, "setPointerCapture", {
      configurable: true,
      value: vi.fn(),
    });
    localStorage.clear();
    globalThis.ResizeObserver = FakeResizeObserver;
  });

  afterEach(() => {
    cleanup();
    computedStyleSpy.mockRestore();
    delete (Element.prototype as { setPointerCapture?: unknown })
      .setPointerCapture;
    delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
    localStorage.clear();
  });

  describe("AppShell 链路", () => {
    it("面板按钮可进入/退出 focus，editor 保持挂载但不可交互", () => {
      render(shellProps(<FocusActionProbe />));
      fireResize(2000);

      fireEvent.click(screen.getByTestId("probe-enter"));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("focus");
      expect(
        screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
      ).toBe("true");
      expect(screen.getByTestId("shell-editor")).toBeTruthy();

      fireEvent.click(screen.getByTestId("probe-exit"));
      expect(screen.getByTestId("probe-surface").textContent).toBe("document");
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("sidecar");
    });

    it("focus 中点击文档工作集先退出 focus；点击面板内不退出", () => {
      render(shellProps(<FocusActionProbe />));
      fireResize(2000);
      fireEvent.click(screen.getByTestId("probe-enter"));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );

      fireEvent.pointerDown(screen.getByTestId("shell-tabbar"));
      expect(screen.getByTestId("probe-surface").textContent).toBe("document");

      fireEvent.click(screen.getByTestId("probe-enter"));
      fireEvent.pointerDown(screen.getByTestId("probe-inside"));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );
    });

    it("窄窗口下用户打开 Agent 自动进入 focus；宽窗口则打开侧车", () => {
      const view = render(
        shellProps(<FocusActionProbe />, { aiPanelOpen: false }),
      );
      fireResize(800);

      view.rerender(shellProps(<FocusActionProbe />, { aiPanelOpen: true }));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("focus");

      // 宽窗口下重新打开：侧车
      fireEvent.click(screen.getByTestId("probe-exit"));
      fireResize(2000);
      view.rerender(shellProps(<FocusActionProbe />, { aiPanelOpen: false }));
      view.rerender(shellProps(<FocusActionProbe />, { aiPanelOpen: true }));
      expect(screen.getByTestId("probe-surface").textContent).toBe("document");
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("sidecar");
    });

    it("focus 中关闭侧车意图退出 focus", () => {
      const view = render(shellProps(<FocusActionProbe />));
      fireResize(2000);
      fireEvent.click(screen.getByTestId("probe-enter"));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );

      view.rerender(shellProps(<FocusActionProbe />, { aiPanelOpen: false }));
      expect(screen.getByTestId("probe-surface").textContent).toBe("document");
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("collapsed");
    });

    it("禅模式临时显示文档主平面，退出后恢复 focus 意图", () => {
      const view = render(shellProps(<FocusActionProbe />));
      fireResize(2000);
      fireEvent.click(screen.getByTestId("probe-enter"));
      expect(
        screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
      ).toBe("true");

      view.rerender(shellProps(<FocusActionProbe />, { zen: true }));
      expect(screen.getByTestId("probe-surface").textContent).toBe(
        "assistant_focus",
      );
      expect(
        screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
      ).toBeNull();
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("collapsed");

      view.rerender(shellProps(<FocusActionProbe />));
      expect(
        screen.getByTestId("workspace-main").getAttribute("aria-hidden"),
      ).toBe("true");
      expect(
        screen
          .getByTestId("unified-assistant-dock")
          .getAttribute("data-presentation"),
      ).toBe("focus");
    });
  });

  describe("Header 展开/返回控件", () => {
    it("只显示会话操作，不保留空闲状态或联网徽章", () => {
      const onToggle = vi.fn();
      const view = render(
        <AssistantPanelHeader
          chromeActionsDisabled={false}
          currentSession={null}
          onDeletedCurrentSession={() => {}}
          onNewChat={() => {}}
          onSelectSession={() => {}}
          profile={PROFILE}
          assistantFocus={false}
          onRequestFocusToggle={onToggle}
        />,
      );

      const button = screen.getByRole("button", { name: "展开阅读" });
      expect(button.getAttribute("title")).toBe("展开阅读");
      expect(screen.queryByTestId("agent-status-trigger")).toBeNull();
      expect(screen.queryByText("准备就绪")).toBeNull();
      fireEvent.click(button);
      expect(onToggle).toHaveBeenCalledTimes(1);

      view.rerender(
        <AssistantPanelHeader
          chromeActionsDisabled={false}
          currentSession={null}
          onDeletedCurrentSession={() => {}}
          onNewChat={() => {}}
          onSelectSession={() => {}}
          profile={PROFILE}
          assistantFocus
          onRequestFocusToggle={onToggle}
        />,
      );
      const back = screen.getByRole("button", { name: "返回文档" });
      expect(back.getAttribute("title")).toBe("返回文档");
    });
  });

  describe("focus 内容列（52rem）", () => {
    it("ConversationSurface 在 focus 时使用 ai-focus-column 内容列", () => {
      const ref = { current: null } as RefObject<HTMLDivElement | null>;
      const view = render(
        <ConversationSurface
          messages={[]}
          streaming={false}
          messageListRef={ref}
          onCitationClick={() => {}}
          onQuoteToInput={() => {}}
          assistantFocus={false}
        />,
      );
      expect(screen.getByTestId("ai-message-list").className).not.toContain(
        "ai-focus-column",
      );

      view.rerender(
        <ConversationSurface
          messages={[]}
          streaming={false}
          messageListRef={ref}
          onCitationClick={() => {}}
          onQuoteToInput={() => {}}
          assistantFocus
        />,
      );
      expect(screen.getByTestId("ai-message-list").className).toContain(
        "ai-focus-column",
      );
    });

    it("AssistantComposerDock 在 focus 时使用 ai-focus-column 内容列", () => {
      const textareaRef = {
        current: null,
      } as RefObject<HTMLTextAreaElement | null>;
      const mentionNavDeltaRef = { current: 0 } as MutableRefObject<1 | -1 | 0>;
      const baseProps = {
        composerDisabled: false,
        images: [] as never[],
        input: "",
        displayMentions: [] as never[],
        mentionCandidates: [] as never[],
        mentionHighlight: -1,
        mentionNavDeltaRef,
        mentionOpen: false,
        mentionPrefix: "@" as const,
        mentionQuery: "",
        streaming: false,
        externalBindings: [] as never[],
        selectedExternalBindingIds: [] as string[],
        textareaRef,
        onComposerKeyDown: () => {},
        onCompositionStart: () => {},
        onCompositionEnd: () => {},
        onImagesChange: () => {},
        onExternalBindingToggle: () => {},
        onMentionHighlight: () => {},
        onMentionSelect: () => {},
        onSubmit: () => {},
        onValueChange: () => {},
        onSelect: () => {},
        onStop: () => {},
        contextReferences: [],
        onRemoveContextReference: () => {},
      };
      const view = render(
        <AssistantComposerDock {...baseProps} assistantFocus={false} />,
      );
      expect(screen.getByTestId("ai-input").className).not.toContain(
        "ai-focus-column",
      );

      view.rerender(<AssistantComposerDock {...baseProps} assistantFocus />);
      expect(screen.getByTestId("ai-input").className).toContain(
        "ai-focus-column",
      );
    });
  });
});
