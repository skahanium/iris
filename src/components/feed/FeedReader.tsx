//! 订阅文章阅读器（阶段 4）。
//!
//! 正文应用 `--prose-measure`；打开后标题聚焦；延迟已读（正文可见 1 秒
//! 或发生滚动/键盘阅读动作后标记，可经设置关闭）；远程图片默认占位，
//! 用户按本篇显式加载；外链只经 openExternalHttpsUrl；「保存为笔记」经
//! App 层回调走现有 fileCreate 链路，目标目录/文件名必须在对话框确认。

import { useEffect, useRef, useState } from "react";
import {
  ArrowUpRight,
  Archive,
  ArchiveRestore,
  CheckCheck,
  ExternalLink,
  ImageOff,
  Loader2,
  Save,
  Star,
  TriangleAlert,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { handleFeedLinkClick, renderFeedMarkdown } from "@/lib/feed-reader";
import {
  buildFeedNoteMarkdown,
  isValidFeedNoteFolder,
} from "@/lib/feed-note-export";
import { openExternalHttpsUrl } from "@/lib/ipc";
import { sanitizeNoteFileName } from "@/lib/note-names";
import { toTrustedHtml } from "@/lib/sanitize";
import { cn } from "@/lib/utils";
import type {
  FeedItemDetail,
  FeedItemStatePatch,
  FeedItemSummary,
} from "@/types/ipc";

export interface FeedReaderProps {
  /** 当前工作区可见时才允许焦点和自动已读副作用。 */
  active?: boolean;
  detail: FeedItemDetail | null;
  status: "idle" | "loading" | "ready" | "error";
  errorCode: string | null;
  autoReadEnabled: boolean;
  onAutoReadEnabledChange: (enabled: boolean) => void;
  onRetry: () => void;
  setItemState: (
    itemId: string,
    patch: FeedItemStatePatch,
  ) => void | Promise<void>;
  /** 保存为笔记（App 层执行 fileCreate + 打开）；缺省时不显示入口。 */
  onSaveAsNote?: (
    markdown: string,
    titleHint: string,
    folderPath: string,
  ) => Promise<string>;
}

/** 保存为笔记对话框：目标目录与文件名必须明确确认，失败可重试。 */
function FeedSaveNoteDialog({
  open,
  detail,
  onOpenChange,
  onSaveAsNote,
}: {
  open: boolean;
  detail: FeedItemDetail;
  onOpenChange: (open: boolean) => void;
  onSaveAsNote: FeedReaderProps["onSaveAsNote"];
}) {
  const [title, setTitle] = useState("");
  const [folder, setFolder] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 打开时用文章标题预填文件名。
  useEffect(() => {
    if (open) {
      setTitle(sanitizeNoteFileName(detail.summary.title));
      setFolder("");
      setBusy(false);
      setError(null);
    }
  }, [detail.summary.title, open]);

  const submit = () => {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setError("请输入文件名。");
      return;
    }
    if (!isValidFeedNoteFolder(folder)) {
      setError("目录不能包含非法字符或“..”。");
      return;
    }
    setBusy(true);
    setError(null);
    const markdown = buildFeedNoteMarkdown(detail, new Date().toISOString());
    onSaveAsNote?.(markdown, trimmedTitle, folder.trim())
      .then(() => onOpenChange(false))
      .catch((caught: unknown) => {
        setError(
          caught instanceof Error
            ? caught.message
            : ((caught as { message?: string })?.message ??
                "保存失败，请重试。"),
        );
        setBusy(false);
      });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="feed-save-note-dialog"
        className="sm:max-w-md"
      >
        <DialogHeader>
          <DialogTitle>保存为笔记</DialogTitle>
          <DialogDescription>
            将生成独立 Markdown 副本写入当前笔记库；后续订阅更新不影响此笔记。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <label className="block space-y-1">
            <span className="text-caption text-muted-foreground">文件名</span>
            <Input
              data-testid="feed-save-note-title"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
          </label>
          <label className="block space-y-1">
            <span className="text-caption text-muted-foreground">
              目标目录（留空为笔记库根目录）
            </span>
            <Input
              data-testid="feed-save-note-folder"
              value={folder}
              placeholder="技术/Rust"
              onChange={(event) => setFolder(event.target.value)}
            />
          </label>
          {error ? (
            <p
              data-testid="feed-save-note-error"
              className="text-caption text-warning"
            >
              {error}
            </p>
          ) : null}
        </div>
        <DialogFooter className="gap-2">
          <Button
            type="button"
            variant="ghost"
            data-testid="feed-save-note-cancel"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            取消
          </Button>
          <Button
            type="button"
            data-testid="feed-save-note-confirm"
            onClick={submit}
            disabled={busy}
          >
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
            ) : (
              <Save className="h-4 w-4" aria-hidden="true" />
            )}
            保存并打开
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function FeedReader({
  active = true,
  detail,
  status,
  autoReadEnabled,
  onAutoReadEnabledChange,
  onRetry,
  setItemState,
  onSaveAsNote,
}: FeedReaderProps) {
  const titleRef = useRef<HTMLHeadingElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [remoteImagesAllowed, setRemoteImagesAllowed] = useState(false);
  const [saveNoteOpen, setSaveNoteOpen] = useState(false);
  const summary: FeedItemSummary | null = detail?.summary ?? null;

  // 打开文章：焦点移到标题；正文可见 1 秒或发生阅读动作后延迟已读。
  useEffect(() => {
    if (!active || status !== "ready" || !summary) return;
    titleRef.current?.focus({ preventScroll: true });
    if (summary.isRead || !autoReadEnabled) return;

    let marked = false;
    const markRead = () => {
      if (marked) return;
      marked = true;
      setItemState(summary.id, { isRead: true });
    };
    const timer = window.setTimeout(markRead, 1000);
    const onReadAction = () => {
      window.clearTimeout(timer);
      markRead();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey) return;
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.matches("input, textarea, select, button") ||
          target.isContentEditable)
      ) {
        return;
      }
      if (
        ![
          "ArrowDown",
          "ArrowUp",
          "PageDown",
          "PageUp",
          " ",
          "Home",
          "End",
        ].includes(event.key)
      ) {
        return;
      }
      onReadAction();
    };
    const body = bodyRef.current;
    body?.addEventListener("scroll", onReadAction, { once: true });
    body?.addEventListener("keydown", onKeyDown);
    return () => {
      window.clearTimeout(timer);
      body?.removeEventListener("scroll", onReadAction);
      body?.removeEventListener("keydown", onKeyDown);
    };
  }, [active, autoReadEnabled, status, summary, setItemState]);

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

  if (status === "idle") {
    return (
      <div
        data-testid="feed-reader"
        className="flex h-full min-h-0 flex-1 items-center justify-center bg-background px-6 text-center text-caption text-muted-foreground"
      >
        选择一篇文章开始阅读
      </div>
    );
  }

  if (status === "error") {
    return (
      <div
        data-testid="feed-reader"
        className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-2 bg-background px-6 text-center"
      >
        <TriangleAlert className="h-5 w-5 text-warning" aria-hidden="true" />
        <p className="text-ui text-muted-foreground">文章加载失败</p>
        <p className="text-caption text-muted-foreground/70">
          无法读取这篇文章，请稍后重试。
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

  // `ready` 必须同时持有详情；异常投影退回中性空态，绝不伪装成请求失败。
  if (!detail || !summary) {
    return (
      <div
        data-testid="feed-reader"
        className="flex h-full min-h-0 flex-1 items-center justify-center bg-background px-6 text-center text-caption text-muted-foreground"
      >
        选择一篇文章开始阅读
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
          <button
            type="button"
            data-testid="feed-toggle-star"
            aria-pressed={summary.isStarred}
            className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
            onClick={() =>
              void setItemState(summary.id, { isStarred: !summary.isStarred })
            }
          >
            <Star className="h-3.5 w-3.5" aria-hidden="true" />
            {summary.isStarred ? "取消收藏" : "收藏"}
          </button>
          <button
            type="button"
            data-testid="feed-toggle-archive"
            aria-pressed={summary.isArchived}
            className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
            onClick={() =>
              void setItemState(summary.id, { isArchived: !summary.isArchived })
            }
          >
            {summary.isArchived ? (
              <ArchiveRestore className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <Archive className="h-3.5 w-3.5" aria-hidden="true" />
            )}
            {summary.isArchived ? "取消归档" : "归档"}
          </button>
          <button
            type="button"
            data-testid="feed-toggle-auto-read"
            aria-pressed={autoReadEnabled}
            className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
            onClick={() => {
              const next = !autoReadEnabled;
              onAutoReadEnabledChange(next);
            }}
          >
            自动已读：{autoReadEnabled ? "开" : "关"}
          </button>
          {onSaveAsNote ? (
            <button
              type="button"
              data-testid="feed-save-as-note"
              className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
              onClick={() => setSaveNoteOpen(true)}
            >
              <Save className="h-3.5 w-3.5" aria-hidden="true" />
              保存为笔记
            </button>
          ) : null}
        </div>

        {detail.fulltextStatus === "pending" ||
        detail.fulltextStatus === "fetching" ? (
          <p className="mt-3 text-caption text-muted-foreground">
            正在获取网页正文；当前显示 Feed 摘要。
          </p>
        ) : null}
        {detail.fulltextStatus === "failed" && detail.contentOrigin !== "web" ? (
          <p className="mt-3 text-caption text-muted-foreground">
            此订阅源仅提供摘要，可在浏览器中查看原文。
          </p>
        ) : null}
        {detail.contentOrigin === "web" ? (
          <p className="mt-3 text-caption text-muted-foreground">网页正文</p>
        ) : null}

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
      {detail && onSaveAsNote ? (
        <FeedSaveNoteDialog
          open={saveNoteOpen}
          detail={detail}
          onOpenChange={setSaveNoteOpen}
          onSaveAsNote={onSaveAsNote}
        />
      ) : null}
    </article>
  );
}
