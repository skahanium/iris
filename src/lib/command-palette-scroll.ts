/**
 * 将候选项滚入可视区；在指定 viewport 上手动设置 scrollTop（避免 scrollIntoView 与 Radix 冲突）。
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

  if (overflowTop > 0 && overflowBottom > 0) {
    viewport.scrollTop += direction > 0 ? overflowBottom : -overflowTop;
  } else if (overflowTop > 0) {
    viewport.scrollTop -= overflowTop;
  } else if (overflowBottom > 0) {
    viewport.scrollTop += overflowBottom;
  }
}
