/**
 * 自适应工作区纯布局策略（v1.2.19 Task 2）。
 *
 * 把用户意图（输入）与有效 presentation（输出）分离：
 * - 用户意图可跨 resize 保持，只持久化允许项（Agent 宽度、导航固定偏好、安全目录展开标识）。
 * - 有效 presentation 是宽度预算确定性计算的结果，resize 永不改写意图。
 *
 * 交互契约见 docs/adaptive-workspace.md §3-§5、§9。
 */

/** 主平面：编辑器可见（默认）或 Agent 占据主工作区（编辑器保持挂载）。 */
export type WorkspacePrimarySurface = "document" | "assistant_focus";

/** 文件导航有效 presentation。 */
export type NavigatorPresentation = "closed" | "peek" | "pinned";

/** Agent 有效 presentation。 */
export type AssistantPresentation = "sidecar" | "collapsed" | "focus";

/** 文档保护宽度（rem），与 `--prose-measure: 52rem` 一致，仅作 CSS 缺失时的兜底。 */
export const WORKSPACE_CHROME_DOCUMENT_PROTECTED_REM = 52;
/** 文件导航固定宽度（rem）。 */
export const WORKSPACE_CHROME_NAVIGATOR_PINNED_REM = 18;
/** Agent 侧车最小宽度（rem）。 */
export const WORKSPACE_CHROME_AGENT_MIN_REM = 25;
/** Agent 侧车目标宽度（rem）。 */
export const WORKSPACE_CHROME_AGENT_TARGET_REM = 30;
/** Agent 侧车最大宽度（rem）。 */
export const WORKSPACE_CHROME_AGENT_MAX_REM = 45;

/** 宽度预算（px）。rem 按实际根字号换算，文档保护宽度使用计算后的 `--prose-measure`。 */
export interface WorkspaceChromeBudgets {
  documentProtectedPx: number;
  navigatorPinnedPx: number;
  agentMinPx: number;
  agentTargetPx: number;
  agentMaxPx: number;
}

/** 布局策略输入：用户意图 + 实测尺寸。 */
export interface WorkspaceChromeInputs {
  /** 可用内容宽度（px），来自 AppShell 实际内容宽度，不使用 window.innerWidth。 */
  contentWidthPx: number;
  /** 文档保护宽度（px），读取计算后的 `--prose-measure`；缺失时回退 52rem。 */
  proseMeasurePx: number;
  /** 根字号（px），用于把 rem 预算换算为 px。 */
  rootFontSizePx: number;
  /** 用户是否希望 Agent 侧车开启（resize 不得改写）。 */
  aiPanelOpen: boolean;
  /** 用户是否希望文件导航打开（resize 不得自动打开已关闭的导航）。 */
  navigatorOpen: boolean;
  /** 用户固定偏好（可持久化）。 */
  pinPreferred: boolean;
  /** 用户主平面意图（仅用户主动切换，resize 不自动进入 focus）。 */
  primarySurface: WorkspacePrimarySurface;
  /** 用户保存的 Agent 侧车宽度（px）；null 表示未保存，使用目标宽度。 */
  savedSidecarWidthPx: number | null;
  /** 禅模式临时覆盖：只改变有效 presentation，不覆盖用户意图。 */
  zenMode: boolean;
}

/** 有效 presentation 投影结果。 */
export interface WorkspaceChromeProjection {
  /** 用户主平面意图（禅模式也不改写）。 */
  primarySurface: WorkspacePrimarySurface;
  navigator: NavigatorPresentation;
  assistant: AssistantPresentation;
  /** 侧车实际生效宽度（px）；非 sidecar 时为恢复宽度（已 clamp 到预算）。 */
  sidecarWidthPx: number;
  /** 是否满足固定资格（导航打开 + 固定偏好 + 预算），供固定按钮显示。 */
  pinnedEligible: boolean;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * 按根字号把 rem 预算换算为 px；`proseMeasurePx` 缺省或非法时回退 52rem。
 */
export function computeWorkspaceChromeBudgets(
  rootFontSizePx: number,
  proseMeasurePx?: number,
): WorkspaceChromeBudgets {
  const root =
    Number.isFinite(rootFontSizePx) && rootFontSizePx > 0 ? rootFontSizePx : 16;
  const rem = (value: number): number => Math.round(value * root);
  const documentProtectedPx =
    typeof proseMeasurePx === "number" && proseMeasurePx > 0
      ? Math.round(proseMeasurePx)
      : rem(WORKSPACE_CHROME_DOCUMENT_PROTECTED_REM);
  return {
    documentProtectedPx,
    navigatorPinnedPx: rem(WORKSPACE_CHROME_NAVIGATOR_PINNED_REM),
    agentMinPx: rem(WORKSPACE_CHROME_AGENT_MIN_REM),
    agentTargetPx: rem(WORKSPACE_CHROME_AGENT_TARGET_REM),
    agentMaxPx: rem(WORKSPACE_CHROME_AGENT_MAX_REM),
  };
}

/**
 * 把用户意图投影为有效 presentation。
 *
 * 降级顺序（§4.1）：1. `pinned` 导航退回 `peek`；2. Agent 宽度向最小值收缩；
 * 3. 仍不足时 Agent 退为 `collapsed`。文档保护宽度在任何情形下不被突破。
 * focus 只来自用户主平面意图；resize 不自动进入 focus。
 */
export function projectWorkspaceChrome(
  inputs: WorkspaceChromeInputs,
): WorkspaceChromeProjection {
  const budgets = computeWorkspaceChromeBudgets(
    inputs.rootFontSizePx,
    inputs.proseMeasurePx,
  );
  const contentWidthPx =
    Number.isFinite(inputs.contentWidthPx) && inputs.contentWidthPx > 0
      ? inputs.contentWidthPx
      : 0;

  // 用户侧车宽度：保存值 clamp 到 [min, max]，未保存用目标宽度。
  const desiredSidecarPx = inputs.aiPanelOpen
    ? clamp(
        inputs.savedSidecarWidthPx ?? budgets.agentTargetPx,
        budgets.agentMinPx,
        budgets.agentMaxPx,
      )
    : null;

  // 固定资格（§4.2）：导航打开 + 固定偏好 + 内容宽度 >= 文档 + 有效侧车宽度（若可见）+ 导航固定宽度。
  const visibleAgentPx = desiredSidecarPx ?? 0;
  const pinnedEligible =
    inputs.navigatorOpen &&
    inputs.pinPreferred &&
    contentWidthPx >=
      budgets.documentProtectedPx + visibleAgentPx + budgets.navigatorPinnedPx;

  let navigator: NavigatorPresentation = "closed";
  if (inputs.navigatorOpen) {
    navigator = pinnedEligible ? "pinned" : "peek";
  }

  let assistant: AssistantPresentation;
  let sidecarWidthPx = desiredSidecarPx ?? budgets.agentTargetPx;

  if (inputs.zenMode) {
    // 禅模式只改变有效 presentation，不覆盖用户意图（§5.1）。
    navigator = "closed";
    assistant = "collapsed";
  } else if (inputs.primarySurface === "assistant_focus") {
    assistant = "focus";
  } else if (desiredSidecarPx === null) {
    assistant = "collapsed";
  } else {
    const navAllocationPx =
      navigator === "pinned" ? budgets.navigatorPinnedPx : 0;
    const remainingPx =
      contentWidthPx - budgets.documentProtectedPx - navAllocationPx;
    if (remainingPx >= desiredSidecarPx) {
      assistant = "sidecar";
      sidecarWidthPx = desiredSidecarPx;
    } else if (remainingPx >= budgets.agentMinPx) {
      // Agent 向最小值收缩，文档保护宽度不被突破。
      assistant = "sidecar";
      sidecarWidthPx = remainingPx;
    } else {
      assistant = "collapsed";
    }
  }

  return {
    primarySurface: inputs.primarySurface,
    navigator,
    assistant,
    sidecarWidthPx,
    pinnedEligible,
  };
}

const NAVIGATOR_PIN_STORAGE_KEY = "iris.workspaceChrome.navigatorPinPreferred";

/** 读取导航固定偏好；缺失或损坏数据回退为未固定。 */
export function loadNavigatorPinPreferred(): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return localStorage.getItem(NAVIGATOR_PIN_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

/** 持久化导航固定偏好（v1.2.19 只持久化此项与 Agent 宽度、安全目录展开标识）。 */
export function saveNavigatorPinPreferred(preferred: boolean): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(NAVIGATOR_PIN_STORAGE_KEY, preferred ? "1" : "0");
  } catch {
    return;
  }
}

const EXPANDED_PREFIX = "iris.workspaceChrome.expanded.v1.";

/**
 * vault identity 必须是不可逆的非敏感标识：非空、无路径分隔符/盘符、非 `.`/`..`。
 * 否则目录展开集合只保存在当前进程，不得持久化绝对路径（§9）。
 */
export function isSafeVaultIdentity(identity: string): boolean {
  if (!identity || identity.length > 128) return false;
  if (/[/\\:]/.test(identity)) return false;
  if (identity === "." || identity === "..") return false;
  return true;
}

/** 展开键必须是 vault 相对路径：非空、非绝对路径、无 `..` 逃逸段。 */
export function isSafeExpandedDirectoryKey(key: string): boolean {
  if (!key || key.length > 512) return false;
  if (/^[/\\]/.test(key)) return false;
  if (/^[A-Za-z]:/.test(key)) return false;
  if (key.split(/[/\\]/).some((segment) => segment === "..")) return false;
  return true;
}

function expandedStorageKey(vaultIdentity: string): string {
  return `${EXPANDED_PREFIX}${vaultIdentity}`;
}

/**
 * 读取目录展开集合；vault identity 不安全或数据损坏时返回 null（仅进程内）。
 */
export function loadExpandedDirectories(
  vaultIdentity: string,
): Set<string> | null {
  if (!isSafeVaultIdentity(vaultIdentity)) return null;
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(expandedStorageKey(vaultIdentity));
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    const out = new Set<string>();
    for (const item of parsed) {
      if (typeof item === "string" && isSafeExpandedDirectoryKey(item)) {
        out.add(item);
      }
    }
    return out;
  } catch {
    return null;
  }
}

/**
 * 持久化目录展开集合；vault identity 不安全或值为绝对路径时静默跳过（仅进程内）。
 * 空集合删除存储键。
 */
export function saveExpandedDirectories(
  vaultIdentity: string,
  expanded: ReadonlySet<string>,
): void {
  if (!isSafeVaultIdentity(vaultIdentity)) return;
  if (typeof localStorage === "undefined") return;
  try {
    const safe = [...expanded].filter(isSafeExpandedDirectoryKey).sort();
    const key = expandedStorageKey(vaultIdentity);
    if (safe.length === 0) {
      localStorage.removeItem(key);
    } else {
      localStorage.setItem(key, JSON.stringify(safe));
    }
  } catch {
    return;
  }
}
