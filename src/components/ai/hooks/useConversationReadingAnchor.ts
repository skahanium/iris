import {
  useCallback,
  useEffect,
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

/**
 * Follows the latest streaming content by growing into unused viewport space.
 * Human upward movement detaches until the user returns to the newest output.
 */
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
  const programmaticWriteRef = useRef(false);
  const activeStreamKeyRef = useRef<string | null>(null);
  const lastObservedScrollTopRef = useRef(0);
  const parkNeededRef = useRef(false);

  const returnToLatest = useCallback(() => {
    parkNeededRef.current = false;
    setFollowing(true);
  }, []);

  useLayoutEffect(() => {
    if (!active || !streamKey || activeStreamKeyRef.current === streamKey) {
      return;
    }
    activeStreamKeyRef.current = streamKey;
    parkNeededRef.current = true;
    setFollowing(true);
  }, [active, streamKey]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onScroll = () => {
      const nextScrollTop = viewport.scrollTop;
      const movedUpBy = lastObservedScrollTopRef.current - nextScrollTop;
      const maxScrollTop = Math.max(
        0,
        viewport.scrollHeight - viewport.clientHeight,
      );
      const nearBottom =
        nextScrollTop >= maxScrollTop - DETACH_BOTTOM_THRESHOLD;
      lastObservedScrollTopRef.current = nextScrollTop;
      if (programmaticWriteRef.current) return;
      if (movedUpBy >= DETACH_BOTTOM_THRESHOLD) {
        setFollowing(false);
        return;
      }
      if (nearBottom) {
        setFollowing(true);
      }
    };
    lastObservedScrollTopRef.current = viewport.scrollTop;
    viewport.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      viewport.removeEventListener("scroll", onScroll);
    };
  }, [viewportRef]);

  useLayoutEffect(() => {
    if (!active || !following) return;
    const viewport = viewportRef.current;
    if (!viewport) return;
    const viewportBox = viewport.getBoundingClientRect();
    const live = viewport.querySelector<HTMLElement>("[data-live-stream]");
    const park = viewport.querySelector<HTMLElement>(
      "[data-conversation-park]",
    );
    const spacer = viewport.querySelector<HTMLElement>(
      "[data-conversation-spacer]",
    );
    const toContentY = (rectTop: number) =>
      rectTop - viewportBox.top + viewport.scrollTop;
    const liveBox = live?.getBoundingClientRect();
    const parkBox = park?.getBoundingClientRect();
    const shouldPark = parkNeededRef.current;
    const result = conversationFollowTarget({
      scrollTop: viewport.scrollTop,
      scrollHeight: viewport.scrollHeight,
      clientHeight: viewport.clientHeight,
      liveBottom: liveBox ? toContentY(liveBox.bottom) : null,
      parkTop: parkBox ? toContentY(parkBox.top) : null,
      shouldPark,
      spacerPx:
        spacer?.getBoundingClientRect().height || CONVERSATION_BOTTOM_SPACER_PX,
    });
    if (shouldPark && parkBox) {
      parkNeededRef.current = false;
    }
    if (!result.changed) return;
    programmaticWriteRef.current = true;
    lastObservedScrollTopRef.current = result.scrollTop;
    viewport.scrollTop = result.scrollTop;
    window.requestAnimationFrame(() => {
      programmaticWriteRef.current = false;
    });
  }, [active, following, revision, viewportRef]);

  return { following, returnToLatest };
}
