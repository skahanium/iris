import { readFileSync } from "node:fs";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";

import { afterEach, describe, expect, it, vi } from "vitest";

import { AppStatusBarSlot } from "@/components/layout/AppStatusBarSlot";
import { fileLinkSummary } from "@/lib/ipc";
import { EMPTY_ASSISTANT_CHROME } from "@/types/assistant-chrome";

vi.mock("@/lib/ipc", () => ({
  fileLinkSummary: vi.fn(),
}));

const mockFileLinkSummary = vi.mocked(fileLinkSummary);

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderStatusBarSlot(
  props: Partial<Parameters<typeof AppStatusBarSlot>[0]> = {},
) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  const onOpenKnowledgeRelations = vi.fn();
  act(() => {
    root?.render(
      createElement(AppStatusBarSlot, {
        activePath: "target.md",
        activeDocumentTitle: "Target",
        unsaved: false,
        characterCount: 1200,
        readingMinutes: 6,
        aiStatus: "AI 空闲",
        assistantChrome: EMPTY_ASSISTANT_CHROME,
        editorZoom: 1,
        onEditorZoomIn: () => {},
        onEditorZoomOut: () => {},
        onEditorZoomReset: () => {},
        onEditorZoomChange: () => {},
        onUndo: () => {},
        onRedo: () => {},
        canUndo: false,
        canRedo: false,
        webSearch: false,
        onWebSearchChange: () => {},
        theme: "dark",
        onThemeChange: () => {},
        connectivity: null,
        onOpenConnectivitySettings: () => {},
        onOpenManagementCenter: () => {},
        onOpenGraph: () => {},
        onOpenKnowledgeRelations,
        ...props,
      }),
    );
  });
  return onOpenKnowledgeRelations;
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
  mockFileLinkSummary.mockReset();
});

describe("status bar assistant chrome", () => {
  it("StatusBar accepts assistantChrome and renders token usage", () => {
    const bar = read("src/components/layout/StatusBar.tsx");
    expect(bar).toContain("assistantChrome");
    expect(bar).toContain("StatusBarTokenUsage");
    expect(bar).toContain("toolActivityLabel");
  });

  it("StatusBar keeps the document title as a bounded location hint", () => {
    const bar = read("src/components/layout/StatusBar.tsx");

    expect(bar).toContain("status-bar-document-title");
    expect(bar).toContain("max-w-[min(18rem,32vw)]");
    expect(bar).toContain("title={trimmedTitle || path || undefined}");
    expect(bar).toContain('className="shrink-0 tabular-nums"');
  });

  it("StatusBar does not expose the ordinary-user tool audit entry", () => {
    const bar = read("src/components/layout/StatusBar.tsx");
    expect(bar).not.toContain("status-bar-audit-link");
    expect(bar).not.toContain("工具审计");
    expect(bar).not.toContain("dispatchOpenAuditTrail");
  });

  it("StatusBar never renders classified vault lock state in the global status line", () => {
    const bar = read("src/components/layout/StatusBar.tsx");
    const app = read("src/App.impl.tsx");

    expect(bar).toContain("isClassifiedStatusLine");
    expect(bar).not.toContain("{statusLine}");
    expect(app).not.toContain("涉密保险库已锁定");
  });

  it("StatusBarTokenUsage shows cumulative summary only", () => {
    const token = read("src/components/layout/StatusBarTokenUsage.tsx");
    expect(token).toContain("累计");
    expect(token).not.toContain("本轮");
    expect(token).toContain('data-testid="status-bar-token-usage"');
  });

  it("AiMessageList does not render tool call bubbles", () => {
    const list = read("src/components/ai/AiMessageList.tsx");
    expect(list).not.toContain("ToolCallList");
    expect(list).not.toContain("ToolCallBubble");
  });

  it("UnifiedAssistantPanel does not mount panel token or context status bars", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).not.toContain("TokenUsageBar");
    expect(panel).not.toContain("ContextStatusBar");
    expect(panel).not.toContain("HarnessActivityStrip");
  });

  it("StatusBarSlot loads link counts for the active note and opens knowledge relations", async () => {
    mockFileLinkSummary.mockResolvedValue({
      inboundCount: 2,
      outboundCount: 1,
      inbound: [],
      outbound: [],
    });

    const onOpenKnowledgeRelations = renderStatusBarSlot();

    await act(async () => {
      await Promise.resolve();
    });

    expect(mockFileLinkSummary).toHaveBeenCalledWith("target.md");
    const linkButton = document.querySelector<HTMLButtonElement>(
      '[data-testid="status-bar-link-summary"]',
    );
    expect(linkButton?.textContent).toContain("入链 2");
    expect(linkButton?.textContent).toContain("出链 1");

    act(() => {
      linkButton?.click();
    });

    expect(onOpenKnowledgeRelations).toHaveBeenCalledTimes(1);
  });

  it("StatusBarSlot does not load or render link counts without an active path", async () => {
    mockFileLinkSummary.mockResolvedValue({
      inboundCount: 0,
      outboundCount: 0,
      inbound: [],
      outbound: [],
    });

    renderStatusBarSlot({ activePath: null });

    await act(async () => {
      await Promise.resolve();
    });

    expect(mockFileLinkSummary).not.toHaveBeenCalled();
    expect(
      document.querySelector('[data-testid="status-bar-link-summary"]'),
    ).toBeNull();
  });

  it("StatusBarSlot degrades link counts when summary loading fails", async () => {
    mockFileLinkSummary.mockRejectedValue(new Error("offline"));

    renderStatusBarSlot();

    await act(async () => {
      await Promise.resolve();
    });

    const linkButton = document.querySelector<HTMLButtonElement>(
      '[data-testid="status-bar-link-summary"]',
    );
    expect(linkButton?.textContent).toContain("双链暂不可用");
  });
});
