//! 订阅文章阅读器（阶段 4）。
//!
//! 正文应用 `--prose-measure`；打开后标题聚焦；延迟已读（正文可见 1 秒
//! 或发生滚动/键盘阅读动作后标记，可经设置关闭）；远程图片默认占位，
//! 用户按本篇显式加载；外链只经 openExternalHttpsUrl。

import { useEffect, useRef, useState } from "react";
import {
  ArrowUpRight,
  CheckCheck,
  ExternalLink,
  ImageOff,
  Loader2,
  TriangleAlert,
} from "lucide-react";

import { handleFeedLinkClick, renderFeedMarkdown } from "@/lib/feed-reader";
import { openExternalHttpsUrl } from "@/lib/ipc";
import { toTrustedHtml } from "@/lib/sanitize";
import { cn } from "@/lib/utils";
import type {
  FeedItemDetail,
  FeedItemStatePatch,
  FeedItemSummary,
} from "@/types/ipc";

import { isFeedAutoReadEnabled } from "@/lib/feed-reader";

export interface FeedReaderProps {
  detail: FeedItemDetail | null;
  status: "idle" | "loading" | "ready" | "error";
  errorCode: string | null;
  onRetry: () => void;
  setItemState: (itemId: string, patch: FeedItemStatePatch) => void;
}

export function FeedReader({
  detail,
  status,
  errorCode,
  onRetry,
  setItemState,
}: FeedReaderProps) {
  const titleRef = useRef<HTMLHeadingElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [remoteImagesAllowed, setRemoteImagesAllowed] = useState(false);
  const summary: FeedItemSummary | null = detail?.summary ?? null;
  const autoReadRef = useRef(isFeedAutoReadEnabled());
  autoReadRef.current = isFeedAutoReadEnabled();

  // 打开文章：焦点移到标题；正文可见 1 秒或发生阅读动作后延迟已读。
  useEffect(() => {
    if (status !== "ready" || !summary) return;
    titleRef.current?.focus({ preventScroll: true });
    if (summary.isRead || !autoReadRef.current) return;

    let marked = false;
    const markRead = () => {
      if (marked) return;
      marked = true;
      setItemState(summary.id, { isRead: true });
    };
    const timer = window.setTimeout(markRead, 1000);
    const onScrollOrKey = () => {
      window.clearTimeout(timer);
      markRead();
    };
    const body = bodyRef.current;
    body?.addEventListener("scroll", onScrollOrKey, { once: true });
    window.addEventListener("keydown", onScrollOrKey, { once: true });
    return () => {
      window.clearTimeout(timer);
      body?.removeEventListener("scroll", onScrollOrKey);
      window.removeEventListener("keydown", onScrollOrKey);
    };
  }, [status, summary, setItemState]);

  // 切换文章时重置远程图片加载状态。
  useEffect(() => {
    setRemoteImagesAllowed(false);
  }, [summary?.id]);

  if (status === "loading") {
    return (
      <div
        data-testid="feed-reader"
        className="flex h-full min-h-0 flex-1 items-center justify-center gap-2 bg-background text-caption text-muted-foreground"
      >
        <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
        正在加载文章…
      </div>
    );
  }

  if (status === "error" || !detail || !summary) {
    return (
      <div
        data-testid="feed-reader"
        className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-2 bg-background px-6 text-center"
      >
        <TriangleAlert className="h-5 w-5 text-warning" aria-hidden="true" />
        <p className="text-ui text-muted-foreground">文章加载失败</p>
        <p className="text-caption text-muted-foreground/70">
          {errorCode ?? "feed_item_not_found"}
        </p>
        <button
          type="button"
          data-testid="feed-reader-retry"
          className="iris-focus-soft rounded-md border border-border-subtle px-3 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
          onClick={onRetry}
        >
          重试
        </button>
      </div>
    );
  }

  const markdown = renderFeedMarkdown(
    detail.contentMarkdown,
    remoteImagesAllowed,
  );

  return (
    <article
      data-testid="feed-reader"
      className="h-full min-h-0 flex-1 overflow-y-auto bg-background"
      onClick={(event) => handleFeedLinkClick(event.nativeEvent)}
      ref={bodyRef}
    >
      <div className="mx-auto flex max-w-[var(--prose-measure)] flex-col px-6 py-6">
        <h1
          ref={titleRef}
          tabIndex={-1}
          data-testid="feed-reader-title"
          className="text-2xl font-semibold leading-snug text-foreground focus:outline-none"
        >
          {summary.title}
        </h1>
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground">
          <span>{summary.sourceTitle}</span>
          {summary.publishedAt ? (
            <time dateTime={summary.publishedAt}>
              {new Date(summary.publishedAt).toLocaleDateString("zh-CN")}
            </time>
          ) : null}
          {summary.conversionStatus === "degraded" ? (
            <span
              data-testid="feed-degraded-notice"
              className="rounded-sm border border-border-subtle px-1.5 py-0.5 text-micro"
            >
              转换降级：部分内容可能不完整
            </span>
          ) : null}
        </div>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {summary.canonicalUrl ? (
            <button
              type="button"
              data-testid="feed-open-external"
              className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
              onClick={() => void openExternalHttpsUrl(summary.canonicalUrl!)}
            >
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
              打开原文
            </button>
          ) : null}
          <button
            type="button"
            data-testid="feed-toggle-read"
            className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
            onClick={() =>
              setItemState(summary.id, { isRead: !summary.isRead })
            }
          >
            <CheckCheck className="h-3.5 w-3.5" aria-hidden="true" />
            {summary.isRead ? "标为未读" : "标为已读"}
          </button>
        </div>

        {!remoteImagesAllowed && /feed-img-placeholder/.test(markdown) ? (
          <div className="mt-3 flex items-center gap-2 rounded-md border border-border-subtle bg-panel px-3 py-2 text-caption text-muted-foreground">
            <ImageOff className="h-4 w-4 shrink-0" aria-hidden="true" />
            <span className="flex-1">远程图片默认不加载</span>
            <button
              type="button"
              data-testid="feed-load-remote-images"
              className="iris-focus-soft rounded-md px-2 py-0.5 text-caption text-foreground transition-colors duration-fast hover:bg-muted/60"
              onClick={() => setRemoteImagesAllowed(true)}
            >
              加载本篇图片
            </button>
          </div>
        ) : null}

        <div
          data-testid="feed-reader-body"
          className={cn(
            "prose mt-5 text-body leading-relaxed text-foreground/90",
          )}
          style={{ maxWidth: "var(--prose-measure)" }}
          dangerouslySetInnerHTML={{ __html: toTrustedHtml(markdown) }}
        />

        {summary.canonicalUrl ? (
          <a
            href={summary.canonicalUrl}
            data-testid="feed-reader-permalink"
            className="mt-6 inline-flex items-center gap-1 text-caption text-muted-foreground transition-colors duration-fast hover:text-foreground"
            onClick={(event) => {
              event.preventDefault();
              void openExternalHttpsUrl(summary.canonicalUrl!);
            }}
          >
            <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
            在浏览器中查看原文
          </a>
        ) : null}
      </div>
    </article>
  );
}
