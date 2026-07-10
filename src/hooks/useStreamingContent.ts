import { useRef } from "react";

const MIN_FLUSH_INTERVAL_MS = 80;
const STREAMING_SHORT_CONTENT_LIMIT = 200;
const STREAMING_BIG_JUMP_CHARS = 240;

/**
 * Streaming content throttle for Markdown rendering.
 *
 * During model streaming this protects Markdown 重解析 frequency by returning
 * cached rendered content until at least one of:
 * 1. 80ms has elapsed since the last flush
 * 2. A paragraph break (\\n\\n) is detected
 * 3. Content is shorter than 200 chars (bypass throttle for short text)
 * 4. A large token batch adds at least 240 chars
 *
 * Non-streaming content is always returned immediately.
 */
export function useStreamingContent(
  content: string,
  streaming: boolean,
): string {
  const cacheRef = useRef({ content: "", rendered: "" });
  const lastUpdateRef = useRef(0);

  if (!streaming) {
    if (cacheRef.current.content !== content) {
      cacheRef.current = { content, rendered: content };
    }
    return content;
  }

  const now = performance.now();
  const timeSince = now - lastUpdateRef.current;
  const added = content.length - cacheRef.current.content.length;

  // Content reset or shrink — always update
  if (added < 0) {
    lastUpdateRef.current = now;
    cacheRef.current = { content, rendered: content };
    return content;
  }

  const shortContent = content.length < STREAMING_SHORT_CONTENT_LIMIT;
  const timeUp = timeSince > MIN_FLUSH_INTERVAL_MS;
  const bigJump = added >= STREAMING_BIG_JUMP_CHARS;
  const paragraphBreak =
    content.indexOf("\n\n", cacheRef.current.content.length) >= 0;

  if (shortContent || timeUp || bigJump || paragraphBreak) {
    lastUpdateRef.current = now;
    cacheRef.current = { content, rendered: content };
    return content;
  }

  return cacheRef.current.rendered;
}
