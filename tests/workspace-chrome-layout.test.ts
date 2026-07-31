import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  computeWorkspaceChromeBudgets,
  loadExpandedDirectories,
  loadNavigatorPinPreferred,
  projectWorkspaceChrome,
  saveExpandedDirectories,
  saveNavigatorPinPreferred,
  type WorkspaceChromeInputs,
  type WorkspaceChromeProjection,
} from "@/lib/workspace-chrome-layout";

/** 根字号 16px 下的预算事实：52rem=832、18rem=288、25rem=400、30rem=480、45rem=720 */
const ROOT = 16;
const DOC = 832;
const AGENT_MIN = 400;
const AGENT_TARGET = 480;
const AGENT_MAX = 720;

function project(
  overrides: Partial<WorkspaceChromeInputs> = {},
): WorkspaceChromeProjection {
  return projectWorkspaceChrome({
    contentWidthPx: 2000,
    proseMeasurePx: DOC,
    rootFontSizePx: ROOT,
    aiPanelOpen: true,
    navigatorOpen: false,
    pinPreferred: false,
    primarySurface: "document",
    savedSidecarWidthPx: AGENT_TARGET,
    zenMode: false,
    ...overrides,
  });
}

describe("分辨率与缩放矩阵（Task 8）", () => {
  it.each([
    [1024, "peek", "collapsed"],
    [1366, "peek", "sidecar"],
    [1440, "peek", "sidecar"],
    [1920, "pinned", "sidecar"],
  ])(
    "%ipx 内容宽度：导航 %s / Agent %s（开启导航并固定偏好）",
    (width, navigator, assistant) => {
      const p = project({
        contentWidthPx: width,
        navigatorOpen: true,
        pinPreferred: true,
        savedSidecarWidthPx: 480,
      });
      expect(p.navigator).toBe(navigator);
      expect(p.assistant).toBe(assistant);
      expect(p.sidecarWidthPx).toBeLessThanOrEqual(720);
    },
  );

  it("浏览器缩放/根字号变化后 1440px 布局仍成立", () => {
    // 根字号 18px：52rem=936、18rem=324、25rem=450、30rem=540
    const zoomed = project({
      rootFontSizePx: 18,
      proseMeasurePx: 936,
      contentWidthPx: 1440,
      navigatorOpen: true,
      pinPreferred: true,
      savedSidecarWidthPx: 540,
    });
    // 固定资格需 1800px，1440px 不足 → 导航退回 peek，Agent 收缩到剩余宽度
    expect(zoomed.navigator).toBe("peek");
    expect(zoomed.assistant).toBe("sidecar");
    expect(zoomed.sidecarWidthPx).toBe(1440 - 936);
  });

  it("1920px 下三块区域均不裁切且正文保持保护宽度", () => {
    const p = project({
      contentWidthPx: 1920,
      navigatorOpen: true,
      pinPreferred: true,
      savedSidecarWidthPx: 720,
    });
    // 固定资格：832 + 720 + 288 = 1840 ≤ 1920
    expect(p.pinnedEligible).toBe(true);
    expect(p.navigator).toBe("pinned");
    expect(p.assistant).toBe("sidecar");
    expect(p.sidecarWidthPx).toBe(720);
  });
});
describe("projectWorkspaceChrome 确定性投影", () => {
  it("默认写作态：document + 导航关闭 + Agent 侧车", () => {
    const p = project();
    expect(p.primarySurface).toBe("document");
    expect(p.navigator).toBe("closed");
    expect(p.assistant).toBe("sidecar");
    expect(p.sidecarWidthPx).toBe(AGENT_TARGET);
    expect(p.pinnedEligible).toBe(false);
  });

  it("assistant_focus 时 assistant=focus，且与宽度无关", () => {
    const focus = { primarySurface: "assistant_focus" as const };
    expect(project({ ...focus, contentWidthPx: 2000 }).assistant).toBe("focus");
    expect(project({ ...focus, contentWidthPx: 800 }).assistant).toBe("focus");
    expect(project({ ...focus, contentWidthPx: 0 }).assistant).toBe("focus");
    expect(project({ ...focus, aiPanelOpen: false }).assistant).toBe("focus");
  });

  it("focus 不关闭已打开的导航抽屉，返回文档后恢复", () => {
    const peek = project({
      navigatorOpen: true,
      primarySurface: "assistant_focus",
    });
    expect(peek.navigator).toBe("peek");
    expect(peek.assistant).toBe("focus");
    const back = project({ navigatorOpen: true });
    expect(back.navigator).toBe("peek");
    expect(back.assistant).toBe("sidecar");
  });

  it("导航 presentation：closed / peek / pinned 按意图与预算确定", () => {
    expect(
      project({ navigatorOpen: false, pinPreferred: true }).navigator,
    ).toBe("closed");
    expect(
      project({ navigatorOpen: true, pinPreferred: false }).navigator,
    ).toBe("peek");
    const pinned = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 2000,
    });
    expect(pinned.navigator).toBe("pinned");
    expect(pinned.pinnedEligible).toBe(true);
  });

  it("Agent presentation：sidecar / collapsed 按预算确定", () => {
    expect(project({ contentWidthPx: 2000 }).assistant).toBe("sidecar");
    expect(project({ contentWidthPx: 1200 }).assistant).toBe("collapsed");
    expect(project({ aiPanelOpen: false }).assistant).toBe("collapsed");
  });

  it("用户保存宽度被 clamp 到 25rem–45rem；未保存使用目标宽度", () => {
    expect(project({ savedSidecarWidthPx: 100 }).sidecarWidthPx).toBe(
      AGENT_MIN,
    );
    expect(project({ savedSidecarWidthPx: 2000 }).sidecarWidthPx).toBe(
      AGENT_MAX,
    );
    expect(project({ savedSidecarWidthPx: null }).sidecarWidthPx).toBe(
      AGENT_TARGET,
    );
  });
});

describe("宽度预算降级顺序（§4.1）", () => {
  it("预算充足：pinned + 侧车保持用户宽度", () => {
    const p = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 2000,
      savedSidecarWidthPx: 600,
    });
    expect(p.navigator).toBe("pinned");
    expect(p.assistant).toBe("sidecar");
    expect(p.sidecarWidthPx).toBe(600);
  });

  it("第一步降级：pinned 退回 peek，Agent 宽度不变", () => {
    // 1600 = 832+480+288 恰好不够
    const p = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 1500,
      savedSidecarWidthPx: 480,
    });
    expect(p.navigator).toBe("peek");
    expect(p.assistant).toBe("sidecar");
    expect(p.sidecarWidthPx).toBe(480);
  });

  it("第二步降级：Agent 向最小值收缩，文档保护宽度不被突破", () => {
    // 1232 = 832+400；1250 只够最小宽度 → 收缩到 418
    const p = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 1250,
      savedSidecarWidthPx: 480,
    });
    expect(p.navigator).toBe("peek");
    expect(p.assistant).toBe("sidecar");
    expect(p.sidecarWidthPx).toBe(1250 - DOC);
    expect(p.sidecarWidthPx).toBeGreaterThanOrEqual(AGENT_MIN);
  });

  it("最后降级：Agent 退为 collapsed，文档宽度仍受保护", () => {
    const p = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 1200,
      savedSidecarWidthPx: 480,
    });
    expect(p.assistant).toBe("collapsed");
    expect(p.sidecarWidthPx).toBe(480); // 恢复宽度保留
  });

  it("Agent 关闭时不参与固定资格宽度计算", () => {
    const p = project({
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 1200, // 832+288=1120 足够
      aiPanelOpen: false,
    });
    expect(p.navigator).toBe("pinned");
    expect(p.assistant).toBe("collapsed");
  });

  it("固定资格按有效侧车宽度计算，失去资格立即退回 peek，重新获得可恢复 pinned", () => {
    const wide = {
      navigatorOpen: true,
      pinPreferred: true,
      contentWidthPx: 2000,
    };
    expect(project(wide).navigator).toBe("pinned");
    expect(project({ ...wide, contentWidthPx: 1500 }).navigator).toBe("peek");
    expect(project({ ...wide, contentWidthPx: 2000 }).navigator).toBe("pinned");
  });
});

describe("resize 不改写用户意图（§3.3 / §4.1）", () => {
  it("宽度变化只改变有效 presentation，不改变用户意图输入", () => {
    const inputs: WorkspaceChromeInputs = {
      contentWidthPx: 1200,
      proseMeasurePx: DOC,
      rootFontSizePx: ROOT,
      aiPanelOpen: true,
      navigatorOpen: true,
      pinPreferred: true,
      primarySurface: "document",
      savedSidecarWidthPx: 480,
      zenMode: false,
    };
    const narrow = projectWorkspaceChrome(inputs);
    expect(narrow.assistant).toBe("collapsed");
    const wide = projectWorkspaceChrome({ ...inputs, contentWidthPx: 2000 });
    expect(wide.navigator).toBe("pinned");
    expect(wide.assistant).toBe("sidecar");
  });

  it("resize 不自动进入 focus", () => {
    for (const width of [0, 400, 800, 832, 1200, 2000]) {
      expect(project({ contentWidthPx: width }).assistant).not.toBe("focus");
    }
  });

  it("resize 不自动打开已关闭的导航", () => {
    expect(
      project({ navigatorOpen: false, contentWidthPx: 4000 }).navigator,
    ).toBe("closed");
  });

  it("focus 意图不因 resize 自动退出", () => {
    const focus = { primarySurface: "assistant_focus" as const };
    expect(project({ ...focus, contentWidthPx: 2000 }).assistant).toBe("focus");
    expect(project({ ...focus, contentWidthPx: 800 }).assistant).toBe("focus");
  });

  it("禅模式只改变有效 presentation，不覆盖用户意图", () => {
    const zen = project({
      navigatorOpen: true,
      pinPreferred: true,
      aiPanelOpen: true,
      contentWidthPx: 2000,
      zenMode: true,
    });
    expect(zen.navigator).toBe("closed");
    expect(zen.assistant).toBe("collapsed");
    const zenFocus = project({
      primarySurface: "assistant_focus",
      zenMode: true,
      contentWidthPx: 2000,
    });
    expect(zenFocus.assistant).toBe("collapsed");
    expect(zenFocus.primarySurface).toBe("assistant_focus");
  });
});

describe("computeWorkspaceChromeBudgets（§4）", () => {
  it("根字号变化后按 rem 重新换算预算", () => {
    const budgets = computeWorkspaceChromeBudgets(20, 1040);
    expect(budgets.documentProtectedPx).toBe(1040);
    expect(budgets.navigatorPinnedPx).toBe(360);
    expect(budgets.agentMinPx).toBe(500);
    expect(budgets.agentTargetPx).toBe(600);
    expect(budgets.agentMaxPx).toBe(900);
  });

  it("--prose-measure 缺失时回退 52rem 兜底", () => {
    const budgets = computeWorkspaceChromeBudgets(16);
    expect(budgets.documentProtectedPx).toBe(52 * 16);
  });

  it("非法根字号回退 16px", () => {
    const budgets = computeWorkspaceChromeBudgets(0);
    expect(budgets.agentTargetPx).toBe(480);
    expect(budgets.documentProtectedPx).toBe(DOC);
  });

  it("投影使用实际 proseMeasurePx 而非假定 832px", () => {
    const p = project({
      rootFontSizePx: 20,
      proseMeasurePx: 1040,
      contentWidthPx: 1040 + 360 + 600, // 恰好满足 pinned
      navigatorOpen: true,
      pinPreferred: true,
      savedSidecarWidthPx: 600,
    });
    expect(p.navigator).toBe("pinned");
  });
});

describe("持久化边界（§9）", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("导航固定偏好 roundtrip，默认未固定", () => {
    expect(loadNavigatorPinPreferred()).toBe(false);
    saveNavigatorPinPreferred(true);
    expect(loadNavigatorPinPreferred()).toBe(true);
    saveNavigatorPinPreferred(false);
    expect(loadNavigatorPinPreferred()).toBe(false);
  });

  it("损坏的固定偏好数据回退为未固定", () => {
    localStorage.setItem("iris.workspaceChrome.navigatorPinPreferred", "oops");
    expect(loadNavigatorPinPreferred()).toBe(false);
  });

  it("目录展开集合以安全 vault identity 隔离 roundtrip", () => {
    expect(loadExpandedDirectories("vault-abc")).toBeNull();
    saveExpandedDirectories("vault-abc", new Set(["a", "b/c"]));
    expect([...(loadExpandedDirectories("vault-abc") ?? [])]).toEqual([
      "a",
      "b/c",
    ]);
    expect(loadExpandedDirectories("vault-other")).toBeNull();
  });

  it("不安全 identity（绝对路径/分隔符）拒绝持久化，仅进程内", () => {
    const unsafeIdentities = [
      "",
      "C:\\Users\\me\\vault",
      "/vault",
      "vault/x",
      "..",
    ];
    for (const identity of unsafeIdentities) {
      saveExpandedDirectories(identity, new Set(["a"]));
      expect(loadExpandedDirectories(identity)).toBeNull();
    }
    expect(localStorage.length).toBe(0);
  });

  it("绝对路径形态的展开键拒绝持久化", () => {
    saveExpandedDirectories(
      "vault-abc",
      new Set(["/abs/path", "C:\\win", "ok"]),
    );
    const loaded = loadExpandedDirectories("vault-abc");
    expect([...(loaded ?? [])]).toEqual(["ok"]);
  });

  it("localStorage 不可用时静默降级，不抛错", () => {
    const original = globalThis.localStorage;
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: undefined,
    });
    try {
      expect(loadNavigatorPinPreferred()).toBe(false);
      expect(loadExpandedDirectories("vault-abc")).toBeNull();
      expect(() => saveNavigatorPinPreferred(true)).not.toThrow();
      expect(() =>
        saveExpandedDirectories("vault-abc", new Set(["a"])),
      ).not.toThrow();
    } finally {
      Object.defineProperty(globalThis, "localStorage", {
        configurable: true,
        value: original,
      });
    }
  });
});
