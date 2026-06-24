import { FileText, Image as ImageIcon, Video } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { mediaRelease, mediaResolve } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { MediaTab } from "@/hooks/useMediaTabs";
import type { MediaResolveResult } from "@/types/ipc";

interface MediaWorkspaceViewProps {
  tab: MediaTab;
}

function MediaKindIcon({ kind }: { kind: MediaTab["mediaKind"] }) {
  if (kind === "image") return <ImageIcon className="h-4 w-4" />;
  if (kind === "video") return <Video className="h-4 w-4" />;
  return <FileText className="h-4 w-4" />;
}

function useMediaLease(path: string) {
  const [resolved, setResolved] = useState<MediaResolveResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showPending, setShowPending] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let handle: string | null = null;
    const pendingTimer = window.setTimeout(() => {
      if (!cancelled) setShowPending(true);
    }, 160);

    setResolved(null);
    setError(null);
    setShowPending(false);

    void mediaResolve(path)
      .then((next) => {
        if (cancelled) {
          void mediaRelease(next.handle);
          return;
        }
        handle = next.handle;
        setResolved(next);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        window.clearTimeout(pendingTimer);
        if (!cancelled) setShowPending(false);
      });

    return () => {
      cancelled = true;
      window.clearTimeout(pendingTimer);
      if (handle) {
        void mediaRelease(handle);
      }
    };
  }, [path]);

  return { error, resolved, showPending };
}

export function MediaWorkspaceView({ tab }: MediaWorkspaceViewProps) {
  const { error, resolved, showPending } = useMediaLease(tab.path);
  const label = resolved?.path ?? tab.path;
  const mimeType = resolved?.mimeType ?? tab.mimeType ?? undefined;

  const metaText = useMemo(() => {
    const parts = [
      mimeType,
      resolved?.sizeBytes ? `${resolved.sizeBytes} B` : null,
    ].filter((part): part is string => Boolean(part));
    return parts.join(" · ");
  }, [mimeType, resolved?.sizeBytes]);

  return (
    <section className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="flex h-11 shrink-0 items-center gap-2 border-b px-4 text-sm">
        <MediaKindIcon kind={tab.mediaKind} />
        <div className="min-w-0 flex-1">
          <div className="truncate font-medium text-foreground">
            {tab.title}
          </div>
          {metaText ? (
            <div className="truncate text-xs text-muted-foreground">
              {metaText}
            </div>
          ) : null}
        </div>
      </header>
      <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-auto bg-muted/20">
        {resolved ? (
          <>
            {tab.mediaKind === "image" ? (
              <img
                className="max-h-full max-w-full object-contain"
                src={resolved.url}
                alt={tab.title}
                decoding="async"
              />
            ) : null}
            {tab.mediaKind === "video" ? (
              <video
                className="max-h-full max-w-full"
                src={resolved.url}
                controls
                preload="metadata"
              />
            ) : null}
            {tab.mediaKind === "pdf" ? (
              <object
                className="h-full w-full bg-background"
                data={resolved.url}
                type="application/pdf"
                aria-label={tab.title}
              />
            ) : null}
          </>
        ) : (
          <div
            className={cn(
              "px-4 text-sm text-muted-foreground",
              !showPending && !error && "opacity-0",
            )}
            role={error ? "alert" : "status"}
          >
            {error ? `无法打开媒体：${error}` : `正在准备 ${label}`}
          </div>
        )}
      </div>
    </section>
  );
}
