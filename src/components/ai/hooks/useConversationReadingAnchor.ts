import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

const DETACH_BOTTOM_THRESHOLD = 24;

export interface ReadingAnchorInput {
  scrollHeight: number;
  clientHeight: number;
  tailBottom: number;
}

export interface TailBottomGeometry {
  viewportTop: number;
  viewportScrollTop: number;
  tailBottom: number;
}

/** Maps a viewport-relative tail edge into scroll-content coordinates. */
export function tailBottomInScrollContent({
  viewportTop,
  viewportScrollTop,
  tailBottom,
}: TailBottomGeometry): number {
  return tailBottom - viewportTop + viewportScrollTop;
}

/**
 * Calculates the scroll position that keeps the newest output above the
 * viewport bottom edge.
 *
 * `AiMessageList` reserves a bottom spacer after the virtual rows and the
 * live streaming footer, so scrolling to `maxScrollTop` leaves that spacer
 * visible below the latest assistant content. The tail geometry is accepted
 * for callers that still locate the streaming tail, but the anchor itself
 * intentionally follows the scroll content bottom rather than pinning the
 * tail to the edge.
 */
export function readingAnchorTarget({
  scrollHeight,
  clientHeight,
  tailBottom: _tailBottom,
}: ReadingAnchorInput): number {
  return Math.max(0, scrollHeight - clientHeight);
}

export const CONVERSATION_PARK_PADDING_PX = 12;
export const CONVERSATION_BOTTOM_SPACER_PX = 96;

export function conversationFollowStreamKey({
  live,
  userClientRequestId,
  userRunId,
  activeStreamKey,
  pendingInputRunId,
}: {
  live: boolean;
  userClientRequestId?: string;
  userRunId?: string;
  activeStreamKey: string | null;
  pendingInputRunId?: string | null;
}): string | null {
  if (live) {
    return (
      userClientRequestId ??
      userRunId ??
      activeStreamKey ??
      pendingInputRunId ??
      null
    );
  }
  return activeStreamKey ?? pendingInputRunId ?? null;
}

export interface ConversationFollowInput {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  liveBottom: number | null;
  parkTop: number | null;
  shouldPark: boolean;
  spacerPx?: number;
  parkPaddingPx?: number;
}

/**
 * Follows live output by growing into unused viewport space. The canvas only
 * moves when the live block would cross the bottom spacer, and then only by
 * that overflow. A new turn may park the outgoing user message near the top.
 */
export function conversationFollowTarget({
  scrollTop,
  scrollHeight,
  clientHeight,
  liveBottom,
  parkTop,
  shouldPark,
  spacerPx = CONVERSATION_BOTTOM_SPACER_PX,
  parkPaddingPx = CONVERSATION_PARK_PADDING_PX,
}: ConversationFollowInput): { scrollTop: number; changed: boolean } {
  const maxScroll = Math.max(0, scrollHeight - clientHeight);
  const clamp = (value: number) => Math.max(0, Math.min(maxScroll, value));

  if (shouldPark && parkTop != null) {
    const parked = clamp(parkTop - parkPaddingPx);
    return {
      scrollTop: parked,
      changed: Math.abs(parked - scrollTop) >= 1,
    };
  }

  if (liveBottom == null) {
    return { scrollTop, changed: false };
  }

  const limit = scrollTop + clientHeight - spacerPx;
  if (liveBottom <= limit + 1) {
    return { scrollTop, changed: false };
  }

  const next = clamp(scrollTop + (liveBottom - limit));
  return {
    scrollTop: next,
    changed: Math.abs(next - scrollTop) >= 1,
  };
}

/** Compute enough temporary space to make the new user row physically parkable. */
export function conversationSpacerHeight(
  clientHeight: number,
  contentBottom: number,
  parkTop: number | null,
): number {
  return Math.max(
    CONVERSATION_BOTTOM_SPACER_PX,
    parkTop == null
      ? 0
      : clientHeight - (contentBottom - parkTop) - CONVERSATION_PARK_PADDING_PX,
  );
}

/** Owns every conversation scroll write and observes outer geometry only. */
export function useConversationReadingAnchor({
  viewportRef,
  active,
  revision,
  streamKey,
}: {
  viewportRef: RefObject<HTMLDivElement | null>;
  active: boolean;
  revision: number;
  streamKey: string | null;
}) {
  const [following, setFollowing] = useState(true);
  const followingRef = useRef(true);
  const followGeometryRef = useRef(active);
  const activeStreamKeyRef = useRef<string | null>(null);
  const parkNeededRef = useRef(false);
  const latestNeededRef = useRef(false);
  const scheduleRef = useRef<() => void>(() => undefined);
  const scheduleGeometry = useCallback(() => scheduleRef.current(), []);
  const returnToLatest = useCallback(() => {
    followGeometryRef.current = true;
    followingRef.current = true;
    parkNeededRef.current = false;
    latestNeededRef.current = true;
    setFollowing(true);
    scheduleRef.current();
  }, []);

  useLayoutEffect(() => {
    // Remote/local completion does not revoke an established reading-follow
    // intent: the final RO batch and later image/font changes still need it.
    followGeometryRef.current ||= active;
    if (active && streamKey && activeStreamKeyRef.current !== streamKey) {
      activeStreamKeyRef.current = streamKey;
      parkNeededRef.current = true;
      followingRef.current = true;
      setFollowing(true);
    }
    scheduleRef.current();
  }, [active, revision, streamKey]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current!;
    if (!viewport) return;
    const content =
      viewport.querySelector<HTMLElement>("[data-conversation-content]") ??
      viewport.firstElementChild;
    let frame: number | null = null;
    let disposed = false;
    let expectedScrollTop: number | null = null;
    let lastScrollTop = viewport.scrollTop;
    let lastMaxScrollTop = Math.max(
      0,
      viewport.scrollHeight - viewport.clientHeight,
    );
    let upwardDistance = 0;
    let downwardIntent = false;
    let lastTouchY: number | null = null;
    let previousTime = 0;
    let movementTarget: number | null = null;
    let parking = false;
    let anchor: {
      node: HTMLElement;
      offset: number;
      row: HTMLElement | null;
      rowOffset: number;
      blockKey: string | null;
      sourceKey: string | null;
    } | null = null;
    const reducedMotion = () =>
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    const captureAnchor = () => {
      const box = viewport.getBoundingClientRect();
      const blocks = Array.from(
        viewport.querySelectorAll<HTMLElement>("[data-markdown-block]"),
      );
      const rows = Array.from(
        viewport.querySelectorAll<HTMLElement>("[data-conversation-row]"),
      );
      const visible = (element: HTMLElement) => {
        const rect = element.getBoundingClientRect();
        return (
          rect.bottom > box.top + CONVERSATION_PARK_PADDING_PX &&
          rect.top < box.bottom
        );
      };
      const innerRows = Array.from(
        viewport.querySelectorAll<HTMLElement>("[data-source-row]"),
      );
      const node =
        innerRows.find(visible) ?? blocks.find(visible) ?? rows.find(visible);
      if (node) {
        const row = node.closest<HTMLElement>("[data-conversation-row]");
        anchor = {
          node,
          offset: node.getBoundingClientRect().top - box.top,
          row,
          rowOffset: (row?.getBoundingClientRect().top ?? box.top) - box.top,
          blockKey:
            node.getAttribute("data-markdown-block") ??
            node
              .closest("[data-markdown-block]")
              ?.getAttribute("data-markdown-block") ??
            null,
          sourceKey: node.getAttribute("data-source-row"),
        };
      }
    };
    const schedule = () => {
      if (frame === null && !disposed)
        frame = window.requestAnimationFrame(updateGeometry);
    };
    const calibrateGeometryClamp = (top: number) => {
      const maximum = Math.max(
        0,
        viewport.scrollHeight - viewport.clientHeight,
      );
      const clamped =
        maximum < lastMaxScrollTop - 1 &&
        top < lastScrollTop &&
        Math.abs(top - maximum) < 1;
      lastMaxScrollTop = maximum;
      if (clamped) {
        expectedScrollTop = top;
        lastScrollTop = top;
        upwardDistance = 0;
      }
      return clamped;
    };
    function updateGeometry(time: number) {
      frame = null;
      if (disposed) return;
      viewport.dispatchEvent(new Event("iris-conversation-geometry"));
      const viewportBox = viewport.getBoundingClientRect();
      const scrollTop = viewport.scrollTop;
      calibrateGeometryClamp(scrollTop);
      const toContentY = (top: number) => top - viewportBox.top + scrollTop;
      const park = viewport.querySelector<HTMLElement>(
        "[data-conversation-park]",
      );
      const spacer = viewport.querySelector<HTMLElement>(
        "[data-conversation-spacer]",
      );
      const liveRows =
        viewport.querySelectorAll<HTMLElement>("[data-live-stream]");
      const live = liveRows[liveRows.length - 1];
      const parkTop = park
        ? toContentY(park.getBoundingClientRect().top)
        : null;
      const contentBottom = content
        ? toContentY(content.getBoundingClientRect().bottom)
        : 0;
      const liveBottom = live
        ? toContentY(live.getBoundingClientRect().bottom)
        : contentBottom;
      const previousSpacer =
        spacer?.getBoundingClientRect().height ||
        Number.parseFloat(spacer?.style.height ?? "") ||
        CONVERSATION_BOTTOM_SPACER_PX;
      const nextSpacer = conversationSpacerHeight(
        viewport.clientHeight,
        contentBottom,
        parkTop,
      );
      const scrollHeight =
        viewport.scrollHeight + (spacer ? nextSpacer - previousSpacer : 0);
      const maxScroll = Math.max(0, scrollHeight - viewport.clientHeight);
      let target = scrollTop;
      if (!followingRef.current) {
        movementTarget = null;
        if (anchor) {
          let node = anchor.node.isConnected ? anchor.node : null;
          if (!node && anchor.row?.isConnected) {
            const scope =
              anchor.blockKey == null
                ? anchor.row
                : Array.from(
                    anchor.row.querySelectorAll<HTMLElement>(
                      "[data-markdown-block]",
                    ),
                  ).find(
                    (element) =>
                      element.getAttribute("data-markdown-block") ===
                      anchor?.blockKey,
                  );
            node =
              anchor.sourceKey == null
                ? (scope ?? null)
                : (Array.from(
                    scope?.querySelectorAll<HTMLElement>("[data-source-row]") ??
                      [],
                  ).find(
                    (element) =>
                      element.getAttribute("data-source-row") ===
                      anchor?.sourceKey,
                  ) ?? null);
          }
          if (node) {
            anchor.node = node;
            target +=
              node.getBoundingClientRect().top -
              viewportBox.top -
              anchor.offset;
          } else if (anchor.row?.isConnected)
            target +=
              anchor.row.getBoundingClientRect().top -
              viewportBox.top -
              anchor.rowOffset;
        }
      } else if (latestNeededRef.current) {
        parking = true;
        movementTarget = maxScroll;
        latestNeededRef.current = false;
      } else if (followGeometryRef.current && !parking) {
        const result = conversationFollowTarget({
          scrollTop,
          scrollHeight,
          clientHeight: viewport.clientHeight,
          liveBottom,
          parkTop,
          shouldPark: parkNeededRef.current,
          spacerPx: CONVERSATION_BOTTOM_SPACER_PX,
        });
        if (parkNeededRef.current && parkTop != null) {
          parking = result.changed;
          parkNeededRef.current = false;
        }
        if (result.changed) movementTarget = result.scrollTop;
      }
      let settledPark = false;
      const elapsed = previousTime
        ? Math.min(64, Math.max(1, time - previousTime))
        : 16;
      previousTime = time;
      if (followingRef.current && movementTarget != null) {
        movementTarget = Math.max(0, Math.min(maxScroll, movementTarget));
        const delta = movementTarget - scrollTop;
        target =
          reducedMotion() || Math.abs(delta) < 1
            ? movementTarget
            : scrollTop + delta * Math.min(1, elapsed / 70);
        if (Math.abs(movementTarget - target) < 1) {
          target = movementTarget;
          movementTarget = null;
          settledPark = parking;
          parking = false;
        }
      }
      target = Math.max(0, Math.min(maxScroll, target));
      // Complete the read batch before writing either spacer or scroll position.
      if (spacer && Math.abs(previousSpacer - nextSpacer) >= 1)
        spacer.style.height = `${nextSpacer}px`;
      if (Math.abs(target - scrollTop) >= 0.1) {
        viewport.scrollTop = target;
        // Layout may clamp a requested target while virtual rows/spacer settle.
        // Calibrate to the browser's actual landing point before its scroll event.
        const actualScrollTop = viewport.scrollTop;
        expectedScrollTop = actualScrollTop;
        lastScrollTop = actualScrollTop;
        lastMaxScrollTop = Math.max(
          0,
          viewport.scrollHeight - viewport.clientHeight,
        );
      }
      // A park may finish after the final height notification was consumed.
      // Give its new viewport position one overflow pass before going idle.
      if (followingRef.current && (movementTarget != null || settledPark))
        schedule();
    }
    const detach = () => {
      followingRef.current = false;
      movementTarget = null;
      parking = false;
      expectedScrollTop = null;
      parkNeededRef.current = false;
      latestNeededRef.current = false;
      downwardIntent = false;
      setFollowing(false);
      captureAnchor();
    };
    const onScroll = () => {
      const top = viewport.scrollTop;
      if (calibrateGeometryClamp(top)) schedule();
      if (expectedScrollTop != null && Math.abs(expectedScrollTop - top) < 1) {
        expectedScrollTop = null;
        lastScrollTop = top;
        return;
      }
      const movedUp = lastScrollTop - top;
      lastScrollTop = top;
      upwardDistance = movedUp > 0 ? upwardDistance + movedUp : 0;
      if (upwardDistance >= DETACH_BOTTOM_THRESHOLD) detach();
      else if (
        !followingRef.current &&
        downwardIntent &&
        movedUp < 0 &&
        top >=
          viewport.scrollHeight -
            viewport.clientHeight -
            DETACH_BOTTOM_THRESHOLD
      ) {
        followGeometryRef.current = true;
        followingRef.current = true;
        setFollowing(true);
        schedule();
      }
      if (movedUp > 0) downwardIntent = false;
      if (!followingRef.current) captureAnchor();
    };
    const onWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) detach();
      else if (event.deltaY > 0) downwardIntent = true;
    };
    const onTouch = (event: TouchEvent) => {
      detach();
      lastTouchY = event.touches[0]?.clientY ?? null;
    };
    const onTouchMove = (event: TouchEvent) => {
      const y = event.touches[0]?.clientY;
      if (y === undefined) return;
      if (lastTouchY != null && y < lastTouchY) downwardIntent = true;
      else if (lastTouchY != null && y > lastTouchY) detach();
      lastTouchY = y;
    };
    const onKey = (event: KeyboardEvent) => {
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest("input,textarea,[contenteditable='true']")
      )
        return;
      if (
        ["ArrowUp", "PageUp", "Home"].includes(event.key) ||
        (event.key === " " && event.shiftKey)
      )
        detach();
      else if (
        ["ArrowDown", "PageDown", "End"].includes(event.key) ||
        (event.key === " " && !event.shiftKey)
      )
        downwardIntent = true;
    };
    const onPointer = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest('[data-orientation="vertical"]')
      ) {
        detach();
        downwardIntent = true;
      }
    };
    const root = viewport.parentElement;
    scheduleRef.current = schedule;
    viewport.addEventListener("scroll", onScroll, { passive: true });
    viewport.addEventListener("wheel", onWheel, { passive: true });
    viewport.addEventListener("touchstart", onTouch, { passive: true });
    viewport.addEventListener("touchmove", onTouchMove, { passive: true });
    viewport.addEventListener("keydown", onKey);
    root?.addEventListener("pointerdown", onPointer, true);
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(schedule);
    observer?.observe(viewport);
    if (content) observer?.observe(content);
    window.addEventListener("resize", schedule);
    document.fonts?.addEventListener("loadingdone", schedule);
    schedule();
    return () => {
      disposed = true;
      if (frame !== null) window.cancelAnimationFrame(frame);
      scheduleRef.current = () => undefined;
      observer?.disconnect();
      viewport.removeEventListener("scroll", onScroll);
      viewport.removeEventListener("wheel", onWheel);
      viewport.removeEventListener("touchstart", onTouch);
      viewport.removeEventListener("touchmove", onTouchMove);
      viewport.removeEventListener("keydown", onKey);
      root?.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("resize", schedule);
      document.fonts?.removeEventListener("loadingdone", schedule);
    };
  }, [viewportRef]);

  return { following, returnToLatest, scheduleGeometry };
}
