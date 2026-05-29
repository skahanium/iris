/** 侧栏 diff 预览升级阈值：超过则不在正文内联展示大块替换 */
export const SIDEBAR_DIFF_CHAR_THRESHOLD = 480;
export const SIDEBAR_DIFF_LINE_THRESHOLD = 12;

export interface SidebarDiffUpgradeInput {
  originalLength: number;
  replacementLength: number;
  lineDelta?: number;
}

export function shouldUpgradeToSidebarDiff(
  input: SidebarDiffUpgradeInput,
): boolean {
  const lineDelta =
    input.lineDelta ??
    Math.abs(
      estimateLines(input.replacementLength) -
        estimateLines(input.originalLength),
    );

  if (input.replacementLength >= SIDEBAR_DIFF_CHAR_THRESHOLD) {
    return true;
  }
  if (lineDelta >= SIDEBAR_DIFF_LINE_THRESHOLD) {
    return true;
  }
  if (
    input.originalLength > 0 &&
    input.replacementLength / input.originalLength >= 2.5 &&
    input.replacementLength > 200
  ) {
    return true;
  }
  return false;
}

function estimateLines(charCount: number): number {
  if (charCount <= 0) return 0;
  return Math.ceil(charCount / 72);
}

export function countTextLines(text: string): number {
  if (!text) return 0;
  return text.split(/\r?\n/).length;
}

export function patchSpansPreferSidebar(
  patches: Array<{ original_text: string; replacement_text: string }>,
): boolean {
  return patches.some((patch) =>
    shouldUpgradeToSidebarDiff({
      originalLength: patch.original_text.length,
      replacementLength: patch.replacement_text.length,
      lineDelta: Math.abs(
        countTextLines(patch.replacement_text) -
          countTextLines(patch.original_text),
      ),
    }),
  );
}
