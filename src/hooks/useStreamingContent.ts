import { useRef } from "react";

const MIN_FLUSH_INTERVAL_MS = 80;
const STREAMING_SHORT_CONTENT_LIMIT = 200;
const STREAMING_BIG_JUMP_CHARS = 240;

/**
 * 流式输出期间节流渲染内容。
 *
 * 在 streaming=true 时，仅在满足以下条件之一时更新返回值：
 * 1. 新增内容超过 240 字符
 * 2. 距上次更新超过 80ms
 * 3. 遇到段落分隔符（双换行）
 * 4. 总内容不足 200 字符（短文本不节流）
 *
 * 非流式状态下始终返回最新内容。
 *
 * 与下游 useMemo([content]) 配合，降低流式 Markdown 重解析频率，
 * 同时避免 120ms 级固定顿挫。
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
  const bigJump = added > STREAMING_BIG_JUMP_CHARS;
  const timeUp = timeSince > MIN_FLUSH_INTERVAL_MS;
  const paragraphBreak =
    content.indexOf("\n\n", cacheRef.current.content.length) >= 0;

  if (shortContent || bigJump || timeUp || paragraphBreak) {
    lastUpdateRef.current = now;
    cacheRef.current = { content, rendered: content };
    return content;
  }

  return cacheRef.current.rendered;
}
