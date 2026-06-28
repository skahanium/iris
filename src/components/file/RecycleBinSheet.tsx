import { ArchiveRestore, RotateCcw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { recycleList, recyclePurge, recycleRestore } from "@/lib/ipc";
import {
  formatRecycleTimestamp,
  recycleDaysRemaining,
  recycleRetentionLabel,
} from "@/lib/recycle-dates";
import { cn } from "@/lib/utils";
import type { RecycleBinItem } from "@/types/ipc";

interface RecycleBinSheetProps {
  open: boolean;
  onClose: () => void;
  onRestored: (path: string) => void | Promise<void>;
  onIndexChange?: () => void;
}

export function RecycleBinBody({
  open,
  onClose,
  onRestored,
  onIndexChange,
}: RecycleBinSheetProps) {
  const [items, setItems] = useState<RecycleBinItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<RecycleBinItem | null>(
    null,
  );
  const [purgeTarget, setPurgeTarget] = useState<RecycleBinItem | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    void recycleList()
      .then(setItems)
      .catch((e) => setError(e instanceof Error ? e.message : "加载回收站失败"))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (open) {
      refresh();
    }
  }, [open, refresh]);

  const empty = !loading && items.length === 0 && !error;

  const summary = useMemo(() => {
    if (items.length === 0) {
      return null;
    }
    const expiringSoon = items.filter(
      (item) => recycleDaysRemaining(item.expires_at) <= 3,
    ).length;
    return { total: items.length, expiringSoon };
  }, [items]);

  return (
    <>
      <div className="border-b border-border/60 bg-surface-inset/30 px-4 py-3">
        <p className="text-xs leading-relaxed text-muted-foreground">
          已删除的笔记、时间线快照与定稿版本将一并保留{" "}
          <span className="font-medium text-foreground">15 天</span>
          ，到期后自动彻底清除。恢复后将回到原来的路径。
        </p>
        {summary && (
          <p className="mt-2 text-xs text-muted-foreground">
            共 {summary.total} 篇
            {summary.expiringSoon > 0 && (
              <span className="text-muted-foreground">
                {" "}
                · {summary.expiringSoon} 篇即将过期
              </span>
            )}
          </p>
        )}
      </div>

      {error && (
        <p className="px-4 py-2 text-xs text-destructive" role="alert">
          {error}
        </p>
      )}

      <ScrollArea className="min-h-0 flex-1">
        {loading ? (
          <p className="p-4 text-xs text-muted-foreground">加载中…</p>
        ) : empty ? (
          <div className="flex flex-col items-center justify-center gap-3 px-6 py-16 text-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-muted">
              <ArchiveRestore className="h-5 w-5 text-muted-foreground" />
            </div>
            <p className="text-sm font-medium text-foreground">回收站为空</p>
            <p className="max-w-xs text-xs leading-relaxed text-muted-foreground">
              删除笔记后会出现在这里。空白未保存的笔记不会进入回收站。
            </p>
          </div>
        ) : (
          <ul className="py-1">
            {items.map((item) => {
              const daysLeft = recycleDaysRemaining(item.expires_at);
              const urgent = daysLeft <= 3;
              const busy = busyId === item.id;

              return (
                <li
                  key={item.id}
                  className="group border-b border-border/50 last:border-b-0"
                >
                  <div className="flex items-start gap-2 px-3 py-2.5">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium text-foreground">
                        {item.title}
                      </p>
                      <p className="mt-0.5 truncate text-xs text-muted-foreground">
                        {item.original_path}
                      </p>
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-muted-foreground">
                        <span>
                          删除于 {formatRecycleTimestamp(item.deleted_at)}
                        </span>
                        <span aria-hidden>·</span>
                        <span
                          className={cn(
                            urgent &&
                              "font-medium text-amber-600 dark:text-amber-500",
                          )}
                        >
                          {recycleRetentionLabel(daysLeft)}
                        </span>
                        {item.version_count > 0 && (
                          <>
                            <span aria-hidden>·</span>
                            <span>{item.version_count} 个历史版本</span>
                          </>
                        )}
                      </div>
                    </div>
                    <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        className="h-8 w-8 text-muted-foreground hover:text-primary"
                        disabled={busy}
                        aria-label={`恢复 ${item.title}`}
                        title="恢复"
                        onClick={() => setRestoreTarget(item)}
                      >
                        <RotateCcw className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        type="button"
                        size="icon"
                        variant="ghost"
                        className="h-8 w-8 text-muted-foreground hover:text-destructive"
                        disabled={busy}
                        aria-label={`永久删除 ${item.title}`}
                        title="永久删除"
                        onClick={() => setPurgeTarget(item)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </ScrollArea>

      <ConfirmDialog
        open={restoreTarget !== null}
        title="恢复笔记"
        message={
          restoreTarget
            ? restoreTarget.version_count > 0
              ? `将「${restoreTarget.title}」及其 ${restoreTarget.version_count} 个历史版本恢复到 ${restoreTarget.original_path}。若该路径已有文件，恢复将失败。`
              : `将「${restoreTarget.title}」恢复到 ${restoreTarget.original_path}。若该路径已有文件，恢复将失败。`
            : ""
        }
        confirmLabel="恢复"
        onCancel={() => setRestoreTarget(null)}
        onConfirm={() => {
          if (!restoreTarget) return;
          const target = restoreTarget;
          setRestoreTarget(null);
          setBusyId(target.id);
          void (async () => {
            try {
              const path = await recycleRestore(target.id);
              onIndexChange?.();
              refresh();
              await onRestored(path);
              onClose();
            } catch (e) {
              setError(e instanceof Error ? e.message : "恢复失败");
            } finally {
              setBusyId(null);
            }
          })();
        }}
      />

      <ConfirmDialog
        open={purgeTarget !== null}
        title="永久删除"
        message={`确定永久删除「${purgeTarget?.title ?? ""}」？`}
        description="此操作不可撤销，正文、时间线快照与定稿将彻底删除。"
        confirmLabel="永久删除"
        variant="destructive"
        onCancel={() => setPurgeTarget(null)}
        onConfirm={() => {
          if (!purgeTarget) return;
          const target = purgeTarget;
          setPurgeTarget(null);
          setBusyId(target.id);
          void (async () => {
            try {
              await recyclePurge(target.id);
              onIndexChange?.();
              refresh();
            } catch (e) {
              setError(e instanceof Error ? e.message : "删除失败");
            } finally {
              setBusyId(null);
            }
          })();
        }}
      />
    </>
  );
}

export function RecycleBinSheet(props: RecycleBinSheetProps) {
  return (
    <IrisOverlay
      open={props.open}
      onClose={props.onClose}
      title="回收站"
      size="command"
    >
      <RecycleBinBody {...props} />
    </IrisOverlay>
  );
}
