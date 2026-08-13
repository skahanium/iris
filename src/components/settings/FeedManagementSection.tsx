import { useCallback, useEffect, useState } from "react";
import {
  Download,
  HardDrive,
  RefreshCw,
  Rss,
  Settings2,
  Trash2,
} from "lucide-react";

import { FeedOpmlDialog } from "@/components/feed/FeedOpmlDialog";
import { Button } from "@/components/ui/button";
import { useFeedSettings } from "@/hooks/useFeedSettings";
import {
  feedLibraryOptimize,
  feedLibrarySummary,
  feedSyncAll,
  feedTrashClear,
  feedTrashList,
  feedTrashRestore,
} from "@/lib/ipc";
import type { FeedLibrarySummary, FeedTrashItem } from "@/types/ipc";

import {
  PanelSection,
  SettingRow,
  StatusValue,
  SwitchControl,
} from "./managementCenterPrimitives";

const EMPTY_SUMMARY: FeedLibrarySummary = {
  sourceCount: 0,
  enabledSourceCount: 0,
  failedSourceCount: 0,
  itemCount: 0,
  unreadCount: 0,
  lastSuccessAt: null,
};

export function FeedManagementSection({
  proxyStatusLabel,
  onOpenOverview,
}: {
  proxyStatusLabel: string;
  onOpenOverview: () => void;
}) {
  const settings = useFeedSettings();
  const [summary, setSummary] = useState<FeedLibrarySummary>(EMPTY_SUMMARY);
  const [opmlOpen, setOpmlOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [trash, setTrash] = useState<FeedTrashItem[]>([]);
  const [trashOpen, setTrashOpen] = useState(false);

  const refresh = useCallback(() => {
    void feedLibrarySummary()
      .then(setSummary)
      .catch(() => undefined);
  }, []);
  useEffect(refresh, [refresh]);

  const refreshTrash = useCallback(() => {
    void feedTrashList()
      .then(setTrash)
      .catch(() => undefined);
  }, []);

  const syncAll = () => {
    if (syncing) return;
    setSyncing(true);
    setMessage(null);
    void feedSyncAll()
      .then((outcome) => {
        setMessage(
          outcome.failed > 0
            ? `同步结束：${outcome.succeeded} 个成功，${outcome.failed} 个失败。`
            : `同步完成：新增 ${outcome.newItems} 篇文章。${outcome.skippedHistory > 0 ? ` 已略过 ${outcome.skippedHistory} 篇较早历史。` : ""}`,
        );
        refresh();
      })
      .catch(() => setMessage("同步未完成，请稍后重试。"))
      .finally(() => setSyncing(false));
  };

  return (
    <section data-testid="management-section-feeds" className="space-y-5">
      <PanelSection title="阅读与同步">
        <SettingRow
          icon={Rss}
          title="自动已读"
          detail="阅读正文一段时间或发生阅读动作后，自动将文章标为已读。"
        >
          <SwitchControl
            checked={settings.autoReadEnabled}
            label="自动已读"
            data-testid="feed-auto-read-switch"
            onCheckedChange={settings.setAutoReadEnabled}
          />
        </SettingRow>
        <SettingRow
          icon={RefreshCw}
          title="后台自动同步"
          detail="关闭后不再启动新的到期同步；手动同步仍可使用。"
        >
          <SwitchControl
            checked={settings.backgroundSyncEnabled}
            label="后台自动同步"
            data-testid="feed-background-sync-switch"
            onCheckedChange={settings.setBackgroundSyncEnabled}
          />
        </SettingRow>
        <SettingRow
          icon={Settings2}
          title="新订阅默认间隔"
          detail="仅作用于以后手动添加和 OPML 导入的订阅源。"
        >
          <select
            data-testid="feed-default-interval"
            className="h-8 rounded-md border border-border bg-background px-2 text-xs text-foreground"
            value={String(settings.defaultFetchIntervalMinutes)}
            onChange={(event) =>
              settings.setDefaultFetchIntervalMinutes(
                Number(event.target.value),
              )
            }
          >
            <option value="15">15 分钟</option>
            <option value="60">1 小时</option>
            <option value="180">3 小时</option>
            <option value="1440">1 天</option>
          </select>
        </SettingRow>
      </PanelSection>

      <PanelSection title="资料库维护">
        <div className="grid grid-cols-2 gap-2 text-sm sm:grid-cols-3">
          {[
            ["来源", summary.sourceCount],
            ["启用", summary.enabledSourceCount],
            ["失败", summary.failedSourceCount],
            ["文章", summary.itemCount],
            ["未读", summary.unreadCount],
          ].map(([label, value]) => (
            <div
              key={String(label)}
              className="rounded-md border border-border-subtle bg-panel px-3 py-2"
            >
              <p className="text-caption text-muted-foreground">{label}</p>
              <p className="mt-1 font-medium tabular-nums text-foreground">
                {value}
              </p>
            </div>
          ))}
          <div className="rounded-md border border-border-subtle bg-panel px-3 py-2">
            <p className="text-caption text-muted-foreground">最近成功同步</p>
            <p className="mt-1 truncate text-caption text-foreground">
              {summary.lastSuccessAt
                ? new Date(summary.lastSuccessAt).toLocaleString("zh-CN")
                : "尚无记录"}
            </p>
          </div>
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            type="button"
            data-testid="feed-management-sync-all"
            onClick={syncAll}
            disabled={syncing}
          >
            <RefreshCw
              className={syncing ? "h-4 w-4 animate-spin" : "h-4 w-4"}
              aria-hidden="true"
            />
            立即同步全部
          </Button>
          <Button
            type="button"
            variant="outline"
            data-testid="feed-management-opml"
            onClick={() => setOpmlOpen(true)}
          >
            <Download className="h-4 w-4" aria-hidden="true" />
            导入/导出 OPML
          </Button>
        </div>
        {message ? (
          <p role="status" className="mt-2 text-caption text-muted-foreground">
            {message}
          </p>
        ) : null}
        <div className="mt-4 border-t border-border-subtle pt-3">
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              data-testid="feed-management-trash"
              onClick={() => {
                setTrashOpen((open) => !open);
                refreshTrash();
              }}
            >
              <Trash2 className="h-4 w-4" aria-hidden="true" />
              RSS 回收站
            </Button>
            <Button
              type="button"
              variant="ghost"
              data-testid="feed-management-optimize"
              onClick={() => {
                setMessage("正在优化资料库空间…");
                void feedLibraryOptimize()
                  .then(() => setMessage("资料库空间已优化。"))
                  .catch(() => setMessage("资料库优化未完成，请稍后重试。"));
              }}
            >
              <HardDrive className="h-4 w-4" aria-hidden="true" />
              优化资料库空间
            </Button>
          </div>
          {trashOpen ? (
            <div data-testid="feed-trash-list" className="mt-3 space-y-2">
              {trash.length === 0 ? (
                <p className="text-caption text-muted-foreground">
                  RSS 回收站为空。
                </p>
              ) : (
                <>
                  {trash.map((entry) => (
                    <div
                      key={entry.item.id}
                      className="flex items-center gap-2 rounded-md border border-border-subtle px-3 py-2"
                    >
                      <span className="min-w-0 flex-1 truncate text-caption text-foreground">
                        {entry.item.title}
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                          void feedTrashRestore(entry.item.id)
                            .then(() => {
                              refreshTrash();
                              refresh();
                            })
                            .catch(() =>
                              setMessage("恢复未完成，请稍后重试。"),
                            );
                        }}
                      >
                        恢复
                      </Button>
                    </div>
                  ))}
                  <Button
                    type="button"
                    variant="destructive"
                    size="sm"
                    onClick={() => {
                      void feedTrashClear()
                        .then(() => {
                          refreshTrash();
                          refresh();
                        })
                        .catch(() => setMessage("清空未完成，请稍后重试。"));
                    }}
                  >
                    立即清空已删除文章
                  </Button>
                </>
              )}
            </div>
          ) : null}
        </div>
      </PanelSection>

      <PanelSection title="网络">
        <SettingRow
          icon={Rss}
          title="系统代理"
          detail={`当前状态：${proxyStatusLabel}`}
        >
          <StatusValue ready>全局设置</StatusValue>
        </SettingRow>
        <Button type="button" variant="ghost" onClick={onOpenOverview}>
          前往总览
        </Button>
      </PanelSection>
      <FeedOpmlDialog
        open={opmlOpen}
        onOpenChange={setOpmlOpen}
        onSourcesChanged={refresh}
        hasSources={summary.sourceCount > 0}
      />
    </section>
  );
}
