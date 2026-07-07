export interface TabOverflowInput {
  gapPx: number;
  moreButtonPx: number;
  trailingButtonPx?: number;
  railWidthPx: number;
  tabCount: number;
  tabMinPx: number;
}

/**
 * Decide how many tabs the title-bar rail can show before the rest spill into a
 * "更多" overflow menu. Tabs compress to `tabMinPx` once any would overflow; the
 * formula reserves room for the more button (plus its leading gap) and fits as
 * many min-width tabs as the rail width allows, always leaving at least one tab
 * in the menu when the tab set does not fit in full.
 */
export function computeVisibleTabCount({
  gapPx,
  moreButtonPx,
  trailingButtonPx = 0,
  railWidthPx,
  tabCount,
  tabMinPx,
}: TabOverflowInput): number {
  if (tabCount <= 0 || railWidthPx <= 0) {
    return 0;
  }
  const trailingActionWidth =
    trailingButtonPx > 0 ? trailingButtonPx + gapPx : 0;
  const allAtMin =
    tabCount * tabMinPx + (tabCount - 1) * gapPx + trailingActionWidth;
  if (allAtMin <= railWidthPx) {
    return tabCount;
  }
  const usable = railWidthPx - (moreButtonPx + gapPx + trailingActionWidth);
  if (usable <= 0) {
    return 0;
  }
  const count = Math.floor((usable + gapPx) / (tabMinPx + gapPx));
  return Math.max(0, Math.min(count, tabCount - 1));
}
