/** Luminous Rail outline math and scrub helpers. */

export const OUTLINE_LUMINOUS_RAIL_WIDTH = "1.75rem";

export function getTickTop(index: number, total: number): number {
  if (total <= 1) return 50;
  return (index / (total - 1)) * 100;
}

/** Convert a rail index to pixel offset inside the track box. */
export function tickTopPx(
  index: number,
  total: number,
  trackHeight: number,
): number {
  if (trackHeight <= 0) return 0;
  return (getTickTop(index, total) / 100) * trackHeight;
}

/** Fixed viewport coordinates for the floating caption beside the track. */
export function captionCoordsFromTrack(
  trackRect: Pick<DOMRect, "top" | "right" | "height">,
  topPercent: number,
  gapPx = 6,
): { top: number; left: number } {
  return {
    top: trackRect.top + (topPercent / 100) * trackRect.height,
    left: trackRect.right + gapPx,
  };
}

export function clampPointerY(pointerY: number, trackHeight: number): number {
  if (trackHeight <= 0) return 0;
  return Math.max(0, Math.min(trackHeight, pointerY));
}

/** Map pointer Y on the track to the nearest heading index. */
export function nearestIndexFromPointer(
  pointerY: number,
  trackHeight: number,
  total: number,
): number {
  if (total <= 0) return -1;
  if (total === 1) return 0;
  const ratio =
    trackHeight === 0 ? 0 : clampPointerY(pointerY, trackHeight) / trackHeight;
  return Math.round(ratio * (total - 1));
}

/** Advance heading index from wheel delta (one step per notch). */
export function wheelScrubIndex(
  deltaY: number,
  currentIndex: number,
  total: number,
): number {
  if (total <= 0) return -1;
  if (deltaY === 0) return currentIndex;
  const step = deltaY > 0 ? 1 : -1;
  const base = currentIndex < 0 ? 0 : currentIndex;
  return Math.max(0, Math.min(total - 1, base + step));
}

/** Keyboard scrub step for arrow keys. */
export function stepScrubIndex(
  currentIndex: number,
  total: number,
  direction: -1 | 1,
): number {
  if (total <= 0) return -1;
  const base = currentIndex < 0 ? 0 : currentIndex;
  return Math.max(0, Math.min(total - 1, base + direction));
}
