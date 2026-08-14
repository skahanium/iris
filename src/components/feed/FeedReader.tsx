//! 订阅文章阅读器（阶段 4）。
//!
//! 正文应用 `--prose-measure`；打开后标题聚焦；延迟已读（正文可见 1 秒
//! 或发生滚动/键盘阅读动作后标记，可经设置关闭）；远程图片默认占位，
//! 用户按本篇显式加载；外链只经 openExternalHttpsUrl；「保存为笔记」经
//! App 层回调走现有 fileCreate 链路，目标目录/文件名必须在对话框确认。

import { useEffect, useRef, useState } from "react";
import {
  Archive,
  ArchiveRestore,
  CheckCheck,
  ExternalLink,
  ImageOff,
  Loader2,
  FileText,
  X,
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
import { PdfDisplayPanel } from "@/components/layout/PdfDisplayPanel";
import {
  handleFeedImageError,
  handleFeedLinkClick,
  renderFeedMarkdown,
} from "@/lib/feed-reader";
import {
  buildFeedNoteMarkdown,
  isValidFeedNoteFolder,
} from "@/lib/feed-note-export";
import {
  feedDocumentCancel,
  feedDocumentPrepare,
  feedDocumentRelease,
  feedImagePrepare,
  feedImagesAuthorize,
  feedImagesCancel,
  feedImagesRelease,
  listenFeedDocumentProgress,
  openExternalHttpsUrl,
} from "@/lib/ipc";
import { sanitizeNoteFileName } from "@/lib/note-names";
import { toTrustedHtml } from "@/lib/sanitize";
import { cn } from "@/lib/utils";
import type {
  FeedItemDetail,
  FeedItemStatePatch,
  FeedItemSummary,
} from "@/types/ipc";

type ImageLoadState = "queued" | "loading" | "ready" | "failed";

interface QueuedImage {
  index: number;
  forceRetry: boolean;
}

export interface FeedReaderProps {
  /** 当前工作区可见时才允许焦点和自动已读副作用。 */
  active?: boolean;
  detail: FeedItemDetail | null;
  status: "idle" | "loading" | "ready" | "error";
  errorCode: string | null;
  autoReadEnabled: boolean;
  onAutoReadEnabledChange: (enabled: boolean) => void;
  onRetry: () => void;
  /** 对已打开的旧摘要重新请求同一篇网页正文。 */
  onRetryFulltext: () => void;
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
  onRetryFulltext,
  setItemState,
  onSaveAsNote,
}: FeedReaderProps) {
  const titleRef = useRef<HTMLHeadingElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const documentLeaseRef = useRef<{ handle: string; url: string } | null>(null);
  const [imageLeases, setImageLeases] = useState<Map<string, string>>(
    () => new Map(),
  );
  const [imageAuthorizationState, setImageAuthorizationState] = useState<
    "idle" | "loading" | "error"
  >("idle");
  const [imageStates, setImageStates] = useState<Map<number, ImageLoadState>>(
    () => new Map(),
  );
  const imageLeaseHandlesRef = useRef<string[]>([]);
  const imageRequestRef = useRef(0);
  const imageQueueRef = useRef<QueuedImage[]>([]);
  const activeImageLoadsRef = useRef(0);
  const inFlightImageIndicesRef = useRef(new Set<number>());
  const attemptedImageIndicesRef = useRef(new Set<number>());
  const [imageManifest, setImageManifest] = useState<
    Array<{ index: number; sourceUrl: string }>
  >([]);
  const [saveNoteOpen, setSaveNoteOpen] = useState(false);
  const [documentLease, setDocumentLease] = useState<{
    handle: string;
    url: string;
  } | null>(null);
  const [documentStatus, setDocumentStatus] = useState<
    "idle" | "loading" | "error"
  >("idle");
  const [documentBytes, setDocumentBytes] = useState(0);
  const documentRequestRef = useRef(0);
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

  // 切换文章时释放短期 lease。授权和二进制缓存仍按文章持久保存，回来时自动恢复。
  useEffect(() => {
    imageRequestRef.current += 1;
    const imageHandles = imageLeaseHandlesRef.current;
    imageLeaseHandlesRef.current = [];
    imageQueueRef.current = [];
    activeImageLoadsRef.current = 0;
    inFlightImageIndicesRef.current.clear();
    attemptedImageIndicesRef.current.clear();
    setImageManifest([]);
    setImageLeases(new Map());
    setImageStates(new Map());
    setImageAuthorizationState("idle");
    if (imageHandles.length > 0) void feedImagesRelease(imageHandles);
    documentRequestRef.current += 1;
    setDocumentStatus("idle");
    setDocumentBytes(0);
    const lease = documentLeaseRef.current;
    documentLeaseRef.current = null;
    setDocumentLease(null);
    if (lease) void feedDocumentRelease(lease.handle);
    return () => {
      if (summary?.id) {
        void feedDocumentCancel(summary.id);
        void feedImagesCancel(summary.id);
      }
    };
  }, [summary?.id]);

  const setImageState = (index: number, state: ImageLoadState) => {
    setImageStates((current) => {
      const next = new Map(current);
      next.set(index, state);
      return next;
    });
  };

  const drainImageQueue = (itemId: string, request: number) => {
    while (
      activeImageLoadsRef.current < 2 &&
      imageQueueRef.current.length > 0
    ) {
      const queued = imageQueueRef.current.shift();
      if (!queued) break;
      activeImageLoadsRef.current += 1;
      inFlightImageIndicesRef.current.add(queued.index);
      setImageState(queued.index, "loading");
      void feedImagePrepare(itemId, queued.index, queued.forceRetry)
        .then((image) => {
          if (request !== imageRequestRef.current) {
            void feedImagesRelease([image.handle]);
            return;
          }
          imageLeaseHandlesRef.current = [
            ...imageLeaseHandlesRef.current,
            image.handle,
          ];
          setImageLeases((current) => {
            const next = new Map(current);
            next.set(image.sourceUrl, image.url);
            return next;
          });
          setImageState(queued.index, "ready");
        })
        .catch(() => {
          if (request === imageRequestRef.current) {
            setImageState(queued.index, "failed");
          }
        })
        .finally(() => {
          if (request !== imageRequestRef.current) return;
          activeImageLoadsRef.current -= 1;
          inFlightImageIndicesRef.current.delete(queued.index);
          drainImageQueue(itemId, request);
        });
    }
  };

  const queueImages = (
    itemId: string,
    indexes: readonly number[],
    request: number,
    forceRetry: boolean,
  ) => {
    for (const index of indexes) {
      if (!forceRetry && attemptedImageIndicesRef.current.has(index)) continue;
      if (
        inFlightImageIndicesRef.current.has(index) ||
        imageQueueRef.current.some((queued) => queued.index === index)
      ) {
        continue;
      }
      attemptedImageIndicesRef.current.add(index);
      imageQueueRef.current.push({ index, forceRetry });
      setImageState(index, "queued");
    }
    drainImageQueue(itemId, request);
  };

  const prepareImages = (itemId: string) => {
    const request = ++imageRequestRef.current;
    setImageAuthorizationState("loading");
    imageQueueRef.current = [];
    void feedImagesAuthorize(itemId)
      .then((manifest) => {
        if (request !== imageRequestRef.current) return;
        setImageManifest(manifest.images);
        setImageStates(new Map());
        setImageAuthorizationState("idle");
        if (manifest.images.length === 0) {
          return;
        }
        // 首屏先启动两张；其余由下面的 IntersectionObserver 在靠近视口时加入队列。
        queueImages(
          itemId,
          manifest.images.slice(0, 2).map((image) => image.index),
          request,
          false,
        );
      })
      .catch(() => {
        if (request === imageRequestRef.current) {
          setImageAuthorizationState("error");
        }
      });
  };

  const retryImages = (itemId: string, indexes: readonly number[]) => {
    queueImages(itemId, indexes, imageRequestRef.current, true);
  };

  // 已授权文章打开时只从本地缓存恢复；缓存缺失时由同一安全后端补齐。
  useEffect(() => {
    if (detail?.imagesAuthorized && summary?.id) prepareImages(summary.id);
    // The trigger is deliberately article identity + persisted authorization;
    // the queue helpers close over refs so adding them would restart downloads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail?.imagesAuthorized, summary?.id]);

  useEffect(() => {
    if (!summary?.id || imageManifest.length === 0) return;
    const sourceToIndex = new Map(
      imageManifest.map((image) => [image.sourceUrl, image.index]),
    );
    const placeholders = Array.from(
      bodyRef.current?.querySelectorAll<HTMLElement>(
        ".feed-img-placeholder[data-src]",
      ) ?? [],
    );
    const request = imageRequestRef.current;
    const queueForElement = (element: HTMLElement) => {
      const index = sourceToIndex.get(element.dataset.src ?? "");
      if (index !== undefined) queueImages(summary.id, [index], request, false);
    };
    if (!("IntersectionObserver" in window)) {
      placeholders.forEach(queueForElement);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            queueForElement(entry.target as HTMLElement);
            observer.unobserve(entry.target);
          }
        }
      },
      { root: bodyRef.current, rootMargin: "600px 0px" },
    );
    placeholders.forEach((placeholder) => observer.observe(placeholder));
    return () => observer.disconnect();
    // 每个 lease 或单图状态都会重建受控正文 HTML；此时必须转而观察
    // 新占位节点，否则观察器仍指向已脱离 DOM 的旧元素，后续图片不会入队。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imageLeases, imageManifest, imageStates, summary?.id]);

  useEffect(
    () => () => {
      documentRequestRef.current += 1;
      const lease = documentLeaseRef.current;
      documentLeaseRef.current = null;
      if (lease) void feedDocumentRelease(lease.handle);
      const imageHandles = imageLeaseHandlesRef.current;
      imageLeaseHandlesRef.current = [];
      if (imageHandles.length > 0) void feedImagesRelease(imageHandles);
    },
    [],
  );

  useEffect(() => {
    if (!summary?.id) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenFeedDocumentProgress((event) => {
      if (disposed) return;
      if (event.itemId !== summary.id) return;
      setDocumentBytes(event.bytes);
      if (event.status === "failed") setDocumentStatus("error");
      if (event.status === "cancelled") setDocumentStatus("idle");
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
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

  const failedImageSources = new Set(
    imageManifest
      .filter((image) => imageStates.get(image.index) === "failed")
      .map((image) => image.sourceUrl),
  );
  const failedImageIndexes = imageManifest
    .filter((image) => imageStates.get(image.index) === "failed")
    .map((image) => image.index);
  const readyImageCount = imageManifest.filter(
    (image) => imageStates.get(image.index) === "ready",
  ).length;
  const activeImageCount = imageManifest.filter((image) => {
    const state = imageStates.get(image.index);
    return state === "queued" || state === "loading";
  }).length;
  const deferredImageCount = Math.max(
    0,
    imageManifest.length -
      readyImageCount -
      failedImageIndexes.length -
      activeImageCount,
  );

  const markdown = renderFeedMarkdown(
    detail.contentMarkdown,
    detail.imagesAuthorized || imageManifest.length > 0,
    imageLeases,
    failedImageSources,
  );

  const handleReaderClick = (event: React.MouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement;
    const retry = target.closest<HTMLElement>(
      "[data-feed-image-retry][data-src]",
    );
    if (retry) {
      const index = imageManifest.find(
        (image) => image.sourceUrl === retry.dataset.src,
      )?.index;
      if (index !== undefined) {
        event.preventDefault();
        retryImages(summary.id, [index]);
      }
      return;
    }
    handleFeedLinkClick(event.nativeEvent);
  };

  if (documentLease) {
    return (
      <section className="flex h-full min-h-0 flex-1 flex-col bg-background">
        <header className="flex h-11 shrink-0 items-center gap-2 border-b border-border-subtle px-4">
          <FileText className="h-4 w-4" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate text-ui font-medium">
            {summary.title}
          </span>
          <button
            type="button"
            data-testid="feed-document-close"
            className="iris-focus-soft inline-flex h-8 items-center gap-1 rounded-md px-2 text-caption hover:bg-muted/60"
            onClick={() => {
              void feedDocumentRelease(documentLease.handle);
              documentLeaseRef.current = null;
              setDocumentLease(null);
            }}
          >
            <X className="h-4 w-4" aria-hidden="true" />
            返回正文
          </button>
        </header>
        <PdfDisplayPanel
          testId="feed-document-viewer"
          url={documentLease.url}
          label={`${summary.title} PDF`}
        />
      </section>
    );
  }

  return (
    <article
      data-testid="feed-reader"
      className="h-full min-h-0 flex-1 overflow-y-auto bg-background"
      onClick={handleReaderClick}
      onError={(event) => handleFeedImageError(event.nativeEvent)}
      ref={bodyRef}
    >
      <div className="mx-auto flex max-w-[var(--prose-measure)] flex-col px-4 py-6 sm:px-6 sm:py-8">
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
          {detail.primaryDocument?.kind === "pdf" ? (
            <>
              <button
                type="button"
                data-testid="feed-preview-pdf"
                className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-2 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
                disabled={documentStatus === "loading"}
                onClick={() => {
                  const request = ++documentRequestRef.current;
                  setDocumentStatus("loading");
                  setDocumentBytes(0);
                  void feedDocumentPrepare(summary.id)
                    .then((lease) => {
                      if (request !== documentRequestRef.current) {
                        void feedDocumentRelease(lease.handle);
                        return;
                      }
                      const nextLease = {
                        handle: lease.handle,
                        url: lease.url,
                      };
                      documentLeaseRef.current = nextLease;
                      setDocumentLease(nextLease);
                      setDocumentStatus("idle");
                    })
                    .catch(() => {
                      if (request === documentRequestRef.current) {
                        setDocumentStatus("error");
                      }
                    });
                }}
              >
                {documentStatus === "loading" ? (
                  <Loader2
                    className="h-3.5 w-3.5 animate-spin"
                    aria-hidden="true"
                  />
                ) : (
                  <FileText className="h-3.5 w-3.5" aria-hidden="true" />
                )}
                {documentStatus === "loading" ? "正在准备 PDF" : "预览 PDF"}
              </button>
              {documentStatus === "loading" ? (
                <button
                  type="button"
                  data-testid="feed-document-cancel"
                  className="iris-focus-soft inline-flex items-center rounded-md px-2 py-1 text-caption hover:bg-muted/60"
                  onClick={() => {
                    documentRequestRef.current += 1;
                    void feedDocumentCancel(summary.id);
                    setDocumentStatus("idle");
                    setDocumentBytes(0);
                  }}
                >
                  取消下载
                </button>
              ) : null}
            </>
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
        {documentStatus === "error" && detail.primaryDocument ? (
          <div className="mt-3 flex items-center gap-2 text-caption text-muted-foreground">
            <span>PDF 预览未能准备完成。</span>
            <button
              type="button"
              data-testid="feed-open-pdf-external"
              className="iris-focus-soft rounded-md border border-border-subtle px-2 py-0.5 text-foreground"
              onClick={() =>
                void openExternalHttpsUrl(detail.primaryDocument!.url)
              }
            >
              在浏览器中打开 PDF
            </button>
          </div>
        ) : null}
        {documentStatus === "loading" && documentBytes > 0 ? (
          <p className="mt-3 text-caption text-muted-foreground" role="status">
            已下载 {(documentBytes / 1024 / 1024).toFixed(1)} MiB
          </p>
        ) : null}
        {detail.fulltextNeedsRefresh ? (
          <p className="mt-3 text-caption text-muted-foreground">
            正在使用新版规则重新整理网页正文。
          </p>
        ) : null}
        {detail.fulltextStatus === "failed" &&
        detail.contentOrigin !== "web" ? (
          <div className="mt-3 flex items-center gap-2 text-caption text-muted-foreground">
            <span>未能获取网页正文，当前显示 Feed 摘要。</span>
            <button
              type="button"
              data-testid="feed-retry-fulltext"
              className="iris-focus-soft rounded-md border border-border-subtle px-2 py-0.5 text-foreground transition-colors duration-fast hover:bg-muted/60"
              onClick={onRetryFulltext}
            >
              重试获取正文
            </button>
          </div>
        ) : null}
        {detail.contentOrigin === "web" ? (
          <p className="mt-3 text-caption text-muted-foreground">网页正文</p>
        ) : null}

        {/feed-img-placeholder/.test(markdown) ? (
          <div className="mt-3 flex items-center gap-2 rounded-md border border-border-subtle bg-panel px-3 py-2 text-caption text-muted-foreground">
            <ImageOff className="h-4 w-4 shrink-0" aria-hidden="true" />
            <span className="flex-1">
              {imageAuthorizationState === "loading"
                ? detail.imagesAuthorized
                  ? "正在恢复本篇图片"
                  : "正在安全加载本篇图片"
                : imageAuthorizationState === "error"
                  ? "暂时无法准备本篇图片"
                  : failedImageIndexes.length > 0
                    ? `${failedImageIndexes.length} 张图片加载失败`
                    : imageManifest.length > 0
                      ? deferredImageCount > 0
                        ? `已加载 ${readyImageCount}/${imageManifest.length}，继续滚动以加载其余图片`
                        : `正在加载 ${readyImageCount}/${imageManifest.length} 张图片`
                      : "本篇图片尚未加载"}
            </span>
            {failedImageIndexes.length > 0 ? (
              <button
                type="button"
                data-testid="feed-retry-failed-images"
                className="iris-focus-soft rounded-md px-2 py-0.5 text-caption text-foreground transition-colors duration-fast hover:bg-muted/60"
                onClick={() => retryImages(summary.id, failedImageIndexes)}
              >
                重试失败图片
              </button>
            ) : imageManifest.length === 0 ? (
              <button
                type="button"
                data-testid="feed-load-remote-images"
                className="iris-focus-soft rounded-md px-2 py-0.5 text-caption text-foreground transition-colors duration-fast hover:bg-muted/60"
                disabled={imageAuthorizationState === "loading"}
                onClick={() => prepareImages(summary.id)}
              >
                加载本篇图片
              </button>
            ) : null}
          </div>
        ) : null}

        <div
          data-testid="feed-reader-body"
          className={cn("iris-markdown-content mt-5 text-foreground/90")}
          data-prose-surface="feed"
          style={{ maxWidth: "var(--prose-measure)" }}
          dangerouslySetInnerHTML={{ __html: toTrustedHtml(markdown) }}
        />
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
