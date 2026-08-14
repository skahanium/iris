//! 订阅源管理对话框（阶段 4）：添加（发现→确认订阅）与编辑/退订。
//!
//! 添加流程拆为「发现」与「确认订阅」两步，多候选单选、不自动全选；
//! 删除订阅显示文章数并二次确认；保留文章实际将 source 置 disabled。
//! 所有错误只展示稳定错误码文案，不展示 URL/HTTP body/stack。

import { useCallback, useEffect, useState } from "react";
import { Loader2, Plus, Search, TriangleAlert } from "lucide-react";

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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  feedDiscover,
  feedSourceAdd,
  feedSourceTrashMatch,
  feedSourceTrashPreview,
  feedSourceTrash,
  feedSourceUpdate,
  feedSyncSource,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type {
  FeedCandidate,
  FeedSourceSummary,
  FeedTrashSource,
  FeedSourceTrashPreview,
} from "@/types/ipc";

export type FeedSourceDialogMode = "add" | "edit";

export interface FeedSourceDialogProps {
  open: boolean;
  mode: FeedSourceDialogMode;
  /** 编辑目标（mode="edit" 时必填）。 */
  source: FeedSourceSummary | null;
  onOpenChange: (open: boolean) => void;
  /** 添加/编辑/退订完成后刷新源列表。 */
  onSourcesChanged: () => void;
  defaultFetchIntervalMinutes?: number;
}

const INTERVALS = [
  { value: "15", label: "15 分钟" },
  { value: "60", label: "1 小时" },
  { value: "180", label: "3 小时" },
  { value: "1440", label: "1 天" },
];

function FieldError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p
      data-testid="feed-dialog-error"
      className="flex items-center gap-1 text-caption text-warning"
    >
      <TriangleAlert className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      {message}
    </p>
  );
}

/** 发现步骤：URL → 候选单选（不自动全选）。 */
function DiscoverStep({
  onChosen,
}: {
  onChosen: (candidate: FeedCandidate) => void;
}) {
  const [url, setUrl] = useState("");
  const [discovering, setDiscovering] = useState(false);
  const [candidates, setCandidates] = useState<FeedCandidate[]>([]);
  const [selectedUrl, setSelectedUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runDiscover = useCallback(async () => {
    setError(null);
    setDiscovering(true);
    setCandidates([]);
    setSelectedUrl(null);
    try {
      const found = await feedDiscover(url.trim());
      setCandidates(found);
      if (found.length === 0) {
        setError("未找到可订阅的 Feed，请检查网址或稍后重试。");
      }
    } catch (caught) {
      setError(
        (caught as { code?: string })?.code === "feed_validation_url"
          ? "仅支持 HTTPS 地址，且不允许内网地址。"
          : "发现失败，请检查网址后重试。",
      );
    } finally {
      setDiscovering(false);
    }
  }, [url]);

  const chosen = candidates.find((candidate) => candidate.url === selectedUrl);

  return (
    <div className="space-y-3">
      <div data-testid="feed-discover-controls" className="w-full space-y-2">
        <Input
          data-testid="feed-discover-url"
          placeholder="https://example.com/feed.xml 或站点地址"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void runDiscover();
          }}
        />
      </div>
      <FieldError message={error} />
      {candidates.length > 0 ? (
        <fieldset
          data-testid="feed-candidate-list"
          className="space-y-1"
          aria-label="选择要订阅的 Feed"
        >
          {candidates.map((candidate) => (
            <label
              key={candidate.url}
              className={cn(
                "flex cursor-pointer items-start gap-2 rounded-md border border-border-subtle px-3 py-2 transition-colors duration-fast",
                selectedUrl === candidate.url && "border-brand bg-muted/60",
              )}
            >
              <input
                type="radio"
                name="feed-candidate"
                data-testid={`feed-candidate-${candidate.url}`}
                className="mt-1"
                checked={selectedUrl === candidate.url}
                onChange={() => setSelectedUrl(candidate.url)}
              />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-ui">
                  {candidate.title ?? candidate.url}
                </span>
                <span className="block truncate text-caption text-muted-foreground">
                  {candidate.url}
                  {candidate.format ? ` · ${candidate.format}` : ""}
                </span>
              </span>
            </label>
          ))}
        </fieldset>
      ) : null}
      <div className="grid grid-cols-2 gap-2">
        <Button
          type="button"
          data-testid="feed-discover-run"
          variant="outline"
          className="w-full"
          onClick={() => void runDiscover()}
          disabled={discovering || url.trim().length === 0}
        >
          {discovering ? (
            <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          ) : (
            <Search className="h-4 w-4" aria-hidden="true" />
          )}
          发现
        </Button>
        <Button
          type="button"
          data-testid="feed-confirm-subscribe"
          className="w-full"
          disabled={!chosen}
          onClick={() => {
            if (chosen) onChosen(chosen);
          }}
        >
          下一步
        </Button>
      </div>
    </div>
  );
}

/** 确认订阅步骤：标题/分组/间隔/历史未读选项 + 添加并首次同步。 */
function ConfirmStep({
  candidate,
  onBack,
  onDone,
  onSourcesChanged,
  defaultFetchIntervalMinutes,
}: {
  candidate: FeedCandidate;
  onBack: () => void;
  onDone: () => void;
  onSourcesChanged: () => void;
  defaultFetchIntervalMinutes: number;
}) {
  const [title, setTitle] = useState(candidate.title ?? candidate.url);
  const [folderPath, setFolderPath] = useState("");
  const [interval, setInterval] = useState(String(defaultFetchIntervalMinutes));
  const [historyUnread, setHistoryUnread] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [addedSourceId, setAddedSourceId] = useState<string | null>(null);
  const [restoreMatch, setRestoreMatch] = useState<FeedTrashSource | null>(
    null,
  );

  const submit = useCallback(
    async (restoreConfirmed = false) => {
      setBusy(true);
      setError(null);
      setSuccess(null);
      try {
        if (!addedSourceId && !restoreConfirmed) {
          const match = await feedSourceTrashMatch(candidate.url);
          if (match) {
            setRestoreMatch(match);
            return;
          }
        }
        const sourceId =
          addedSourceId ??
          (
            await feedSourceAdd({
              url: candidate.url,
              title: title.trim() || candidate.url,
              folderPath: folderPath.trim() || null,
              fetchIntervalMinutes: Number(interval),
              ...(restoreConfirmed ? { restoreDeleted: true } : {}),
            })
          ).id;
        if (!addedSourceId) setAddedSourceId(sourceId);
        // 首次同步：历史默认已读；勾选「历史也设为未读」时保留未读。
        try {
          const outcome = await feedSyncSource(sourceId, !historyUnread);
          if (outcome.status === "failed") {
            setError("订阅已添加，但首次同步失败；可重试同步或稍后关闭。");
            return;
          }
          if (outcome.skippedHistory > 0) {
            onSourcesChanged();
            setSuccess(
              `订阅已添加：已导入最新 ${outcome.newItems} 篇，略过 ${outcome.skippedHistory} 篇较早历史。`,
            );
            return;
          }
        } catch {
          setError("订阅已添加，但首次同步失败；可重试同步或稍后关闭。");
          return;
        }
        onDone();
      } catch {
        setError("添加订阅失败，请检查设置后重试。");
      } finally {
        setBusy(false);
      }
    },
    [
      addedSourceId,
      candidate.url,
      folderPath,
      historyUnread,
      interval,
      onDone,
      onSourcesChanged,
      title,
    ],
  );

  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border-subtle bg-panel px-3 py-2">
        <span className="block truncate text-caption text-muted-foreground">
          {candidate.url}
        </span>
      </div>
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">标题</span>
        <Input
          data-testid="feed-add-title"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">
          分组（可留空）
        </span>
        <Input
          data-testid="feed-add-folder"
          value={folderPath}
          placeholder="技术/Rust"
          onChange={(event) => setFolderPath(event.target.value)}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">同步间隔</span>
        <Select value={interval} onValueChange={setInterval}>
          <SelectTrigger data-testid="feed-add-interval">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {INTERVALS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="flex items-center gap-2 text-ui">
        <input
          type="checkbox"
          data-testid="feed-add-history-unread"
          checked={historyUnread}
          onChange={(event) => setHistoryUnread(event.target.checked)}
        />
        历史文章也设为未读
      </label>
      <FieldError message={error} />
      {restoreMatch ? (
        <div
          data-testid="feed-restore-match"
          className="rounded-md border border-border-subtle bg-panel px-3 py-2 text-caption text-muted-foreground"
        >
          <p>
            此地址仍在 RSS 回收站中，包含 {restoreMatch.itemCount}{" "}
            篇文章（其中收藏
            {restoreMatch.starredCount}{" "}
            篇）。恢复后将重新启用同步，不会创建重复来源。
          </p>
          <div className="mt-2 flex justify-end gap-2">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setRestoreMatch(null)}
            >
              取消
            </Button>
            <Button
              type="button"
              size="sm"
              data-testid="feed-restore-subscribe"
              onClick={() => void submit(true)}
              disabled={busy}
            >
              恢复并重新订阅
            </Button>
          </div>
        </div>
      ) : null}
      {success ? (
        <div
          data-testid="feed-add-success"
          className="rounded-md border border-border-subtle bg-panel px-3 py-2 text-caption text-muted-foreground"
        >
          <p>{success}</p>
          <Button
            type="button"
            size="sm"
            className="mt-2"
            data-testid="feed-add-success-close"
            onClick={onDone}
          >
            完成
          </Button>
        </div>
      ) : null}
      <DialogFooter className="gap-2 px-0 pb-0">
        <Button type="button" variant="ghost" onClick={onBack} disabled={busy}>
          上一步
        </Button>
        <Button
          type="button"
          data-testid="feed-add-submit"
          onClick={() => void submit(false)}
          disabled={busy || success !== null || restoreMatch !== null}
        >
          {busy ? (
            <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          ) : (
            <Plus className="h-4 w-4" aria-hidden="true" />
          )}
          {addedSourceId ? "重试同步" : "添加并同步"}
        </Button>
      </DialogFooter>
    </div>
  );
}

/** 编辑步骤：覆盖标题/分组/间隔/暂停 + 可恢复退订。 */
function EditStep({
  source,
  onDone,
}: {
  source: FeedSourceSummary;
  onDone: () => void;
}) {
  const [titleOverride, setTitleOverride] = useState(source.title ?? "");
  const [folderPath, setFolderPath] = useState(source.folderPath ?? "");
  const [interval, setInterval] = useState(String(source.fetchIntervalMinutes));
  const [isEnabled, setIsEnabled] = useState(source.isEnabled);
  const [fulltextEnabled, setFulltextEnabled] = useState(
    source.fulltextEnabled ?? true,
  );
  const [trashPreview, setTrashPreview] =
    useState<FeedSourceTrashPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  useEffect(() => {
    if (!source.id) return;
    void feedSourceTrashPreview(source.id)
      .then(setTrashPreview)
      .catch(() => undefined);
  }, [source.id]);

  const save = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await feedSourceUpdate(source.id, {
        titleOverride: titleOverride.trim() || null,
        folderPath: folderPath.trim() || null,
        fetchIntervalMinutes: Number(interval),
        isEnabled,
        fulltextEnabled,
      });
      onDone();
    } catch {
      setError("保存订阅设置失败，请稍后重试。");
    } finally {
      setBusy(false);
    }
  }, [
    folderPath,
    fulltextEnabled,
    interval,
    isEnabled,
    onDone,
    source.id,
    titleOverride,
  ]);

  const remove = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await feedSourceTrash(source.id);
      onDone();
    } catch {
      setError("退订失败，请稍后重试。");
      setBusy(false);
    }
  }, [onDone, source.id]);

  const pause = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await feedSourceUpdate(source.id, { isEnabled: false });
      onDone();
    } catch {
      setError("暂停同步失败，请稍后重试。");
      setBusy(false);
    }
  }, [onDone, source.id]);

  if (confirmingDelete) {
    return (
      <div className="space-y-3">
        <p data-testid="feed-delete-confirm" className="text-ui">
          将订阅源「{source.title}」及其全部文章（共
          {trashPreview?.itemCount ?? "…"} 篇，其中收藏
          {trashPreview?.starredCount ?? "…"} 篇）移入 RSS 回收站。预计于
          {trashPreview
            ? new Date(trashPreview.purgeAfter).toLocaleDateString()
            : "30 天后"}
          清理；在此之前可以整体恢复。已保存为 Markdown 的笔记不受影响，PDF
          临时缓存将释放。
        </p>
        <FieldError message={error} />
        <DialogFooter className="gap-2 px-0 pb-0">
          <Button
            type="button"
            variant="ghost"
            onClick={() => setConfirmingDelete(false)}
            disabled={busy}
          >
            取消
          </Button>
          <Button
            type="button"
            data-testid="feed-delete-confirm-submit"
            variant="destructive"
            onClick={() => void remove()}
            disabled={busy}
          >
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
            ) : null}
            退订并移入 RSS 回收站
          </Button>
        </DialogFooter>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">
          显示标题（留空使用 Feed 原标题）
        </span>
        <Input
          data-testid="feed-edit-title"
          value={titleOverride}
          onChange={(event) => setTitleOverride(event.target.value)}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">分组</span>
        <Input
          data-testid="feed-edit-folder"
          value={folderPath}
          onChange={(event) => setFolderPath(event.target.value)}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-caption text-muted-foreground">同步间隔</span>
        <Select value={interval} onValueChange={setInterval}>
          <SelectTrigger data-testid="feed-edit-interval">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {INTERVALS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="flex items-center gap-2 text-ui">
        <input
          type="checkbox"
          data-testid="feed-edit-enabled"
          checked={isEnabled}
          onChange={(event) => setIsEnabled(event.target.checked)}
        />
        启用同步（关闭即暂停）
      </label>
      <label className="flex items-start gap-2 text-ui">
        <input
          type="checkbox"
          data-testid="feed-edit-fulltext"
          checked={fulltextEnabled}
          onChange={(event) => setFulltextEnabled(event.target.checked)}
        />
        <span>
          <span className="block">自动补全网页正文</span>
          <span className="block text-caption text-muted-foreground">
            默认开启。新摘要自动补全；已保存摘要在打开时按需补全。失败时仍保留
            Feed 摘要和原文链接。
          </span>
        </span>
      </label>
      <FieldError message={error} />
      <DialogFooter className="gap-2 px-0 pb-0">
        <Button
          type="button"
          data-testid="feed-edit-save"
          onClick={() => void save()}
          disabled={busy}
        >
          保存
        </Button>
      </DialogFooter>
      <div className="border-t border-border-subtle pt-3">
        <p className="mb-2 text-caption text-muted-foreground">
          退订（可在 30 天内恢复）
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            data-testid="feed-unsubscribe-keep"
            variant="outline"
            onClick={() => void pause()}
            disabled={busy}
          >
            暂停同步
          </Button>
          <Button
            type="button"
            data-testid="feed-unsubscribe-delete"
            variant="destructive"
            onClick={() => setConfirmingDelete(true)}
            disabled={busy}
          >
            退订并移入 RSS 回收站（{trashPreview?.itemCount ?? "…"} 篇）
          </Button>
        </div>
      </div>
    </div>
  );
}

export function FeedSourceDialog({
  open,
  mode,
  source,
  onOpenChange,
  onSourcesChanged,
  defaultFetchIntervalMinutes = 60,
}: FeedSourceDialogProps) {
  const [step, setStep] = useState<"discover" | "confirm">("discover");
  const [candidate, setCandidate] = useState<FeedCandidate | null>(null);

  // 每次打开重置流程状态。
  useEffect(() => {
    if (open) {
      setStep("discover");
      setCandidate(null);
    }
  }, [open]);

  const handleDone = useCallback(() => {
    onSourcesChanged();
    onOpenChange(false);
  }, [onOpenChange, onSourcesChanged]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent data-testid="feed-source-dialog" className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{mode === "add" ? "添加订阅" : "管理订阅"}</DialogTitle>
          <DialogDescription>
            {mode === "add"
              ? "先发现 Feed 候选，再确认订阅设置。"
              : "修改订阅设置；如不再需要，可退订并移入 RSS 回收站。"}
          </DialogDescription>
        </DialogHeader>
        <div data-testid="feed-source-dialog-body" className="px-5 pb-5">
          {mode === "add" ? (
            step === "discover" ? (
              <DiscoverStep
                onChosen={(chosen) => {
                  setCandidate(chosen);
                  setStep("confirm");
                }}
              />
            ) : candidate ? (
              <ConfirmStep
                candidate={candidate}
                onBack={() => setStep("discover")}
                onDone={handleDone}
                onSourcesChanged={onSourcesChanged}
                defaultFetchIntervalMinutes={defaultFetchIntervalMinutes}
              />
            ) : null
          ) : source ? (
            <EditStep source={source} onDone={handleDone} />
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
