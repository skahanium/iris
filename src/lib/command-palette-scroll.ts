/**
 * 将候选项滚入可视区；优先 `scrollIntoView(nearest)` 减少边界处过度滚动。
 * @param direction 键盘方向；0 表示鼠标悬停，不强制滚动。
 */
export function ensureOptionVisible(
  viewport: HTMLElement,
  el: HTMLElement,
  direction: 1 | -1 | 0 = 0,
) {
  if (direction === 0) {
    return;
  }

  const padding = 8;
  const elRect = el.getBoundingClientRect();
  const vpRect = viewport.getBoundingClientRect();

  const overflowTop = vpRect.top + padding - elRect.top;
  const overflowBottom = elRect.bottom - (vpRect.bottom - padding);

  if (overflowTop <= 0 && overflowBottom <= 0) {
    return;
  }

  if (typeof el.scrollIntoView === "function") {
    el.scrollIntoView({
      block: "nearest",
      inline: "nearest",
      behavior: "auto",
    });
    return;
  }

  if (overflowTop > 0 && overflowBottom > 0) {
    if (direction > 0) {
      viewport.scrollTop += overflowBottom;
    } else {
      viewport.scrollTop -= overflowTop;
    }
    return;
  }

  if (overflowTop > 0) {
    viewport.scrollTop -= overflowTop;
    return;
  }

  if (overflowBottom > 0) {
    viewport.scrollTop += overflowBottom;
  }
}
