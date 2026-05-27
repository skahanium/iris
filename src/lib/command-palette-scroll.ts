/**
 * 仅滚动「刚好露出」候选项的距离，避免贴底对齐导致一次跳过多条。
 * @param direction 键盘方向；0 表示鼠标悬停，取溢出较小的一侧。
 */
export function ensureOptionVisible(
  viewport: HTMLElement,
  el: HTMLElement,
  direction: 1 | -1 | 0 = 0,
) {
  const padding = 8;
  const elRect = el.getBoundingClientRect();
  const vpRect = viewport.getBoundingClientRect();

  const overflowTop = vpRect.top + padding - elRect.top;
  const overflowBottom = elRect.bottom - (vpRect.bottom - padding);

  if (overflowTop <= 0 && overflowBottom <= 0) {
    return;
  }

  if (overflowTop > 0 && overflowBottom > 0) {
    if (direction > 0) {
      viewport.scrollTop += overflowBottom;
    } else if (direction < 0) {
      viewport.scrollTop -= overflowTop;
    } else if (overflowTop >= overflowBottom) {
      viewport.scrollTop -= overflowTop;
    } else {
      viewport.scrollTop += overflowBottom;
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
