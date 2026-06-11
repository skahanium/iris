import {
  Database,
  HardDrive,
  Moon,
  ShieldCheck,
  Sparkles,
  Sun,
  Wifi,
  type LucideIcon,
} from "lucide-react";
import type { ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import { cn } from "@/lib/utils";

interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
  webSearch: boolean;
  onWebSearchChange: (enabled: boolean) => void;
}

function SettingCard({
  title,
  description,
  children,
  className,
}: {
  title: string;
  description: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cn(
        "bg-surface-inset/28 rounded-lg border border-border/65 p-3",
        className,
      )}
    >
      <div className="mb-3">
        <h3 className="text-xs font-medium text-foreground">{title}</h3>
        <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
          {description}
        </p>
      </div>
      {children}
    </section>
  );
}

function StatusRow({
  icon: Icon,
  label,
  value,
  ready,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  ready: boolean;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2 rounded-md border border-border/45 bg-background/35 px-2.5 py-2">
      <Icon className="h-3.5 w-3.5 shrink-0 text-knowledge-foreground" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[11px] text-muted-foreground">
          {label}
        </div>
        <div className="truncate text-xs text-foreground">{value}</div>
      </div>
      <span
        className={cn(
          "size-2 rounded-full",
          ready
            ? "bg-[hsl(var(--status-llm-ready))]"
            : "bg-[hsl(var(--status-inactive)/0.55)]",
        )}
        aria-hidden
      />
    </div>
  );
}

export function SettingsPanel({
  open,
  onClose,
  theme,
  onThemeChange,
  webSearch,
  onWebSearchChange,
}: SettingsPanelProps) {
  const { status } = useConnectivityStatus();
  const searchBackend =
    status?.searchApi.effectiveBackend === "minimax"
      ? "MiniMax"
      : "DuckDuckGo / 本地备用";
  const llmReady = status?.llm.state === "ready";

  return (
    <IrisOverlay open={open} onClose={onClose} title="设置" size="command">
      <ScrollArea className="flex-1">
        <div
          data-testid="settings-control-center"
          className="grid gap-3 px-4 py-4"
        >
          <div className="rounded-lg border border-border/70 bg-surface-elevated/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h2 className="text-sm font-medium text-foreground">
                  工作台偏好
                </h2>
                <p className="mt-1 text-xs text-muted-foreground">
                  让外观、联网与本地数据状态保持在一个安静的控制面板里。
                </p>
              </div>
              <Badge variant="outline" className="bg-background/45">
                本地优先
              </Badge>
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <SettingCard
              title="外观"
              description="快速切换日间与夜间阅读环境。"
            >
              <div className="grid grid-cols-2 gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant={theme === "dark" ? "default" : "outline"}
                  className="justify-center gap-1.5"
                  onClick={() => onThemeChange("dark")}
                >
                  <Moon className="h-3.5 w-3.5" />
                  暗色
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={theme === "light" ? "default" : "outline"}
                  className="justify-center gap-1.5"
                  onClick={() => onThemeChange("light")}
                >
                  <Sun className="h-3.5 w-3.5" />
                  亮色
                </Button>
              </div>
            </SettingCard>

            <SettingCard
              title="联网搜索"
              description="控制助手是否可以使用网络检索工具。"
            >
              <button
                type="button"
                role="switch"
                aria-checked={webSearch}
                className="flex w-full items-center justify-between rounded-md border border-border/55 bg-background/40 px-3 py-2 text-left transition-colors duration-base hover:bg-surface-inset/70 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary"
                onClick={() => onWebSearchChange(!webSearch)}
              >
                <span>
                  <span className="block text-xs font-medium text-foreground">
                    {webSearch ? "已开启" : "已关闭"}
                  </span>
                  <span className="mt-0.5 block text-[11px] text-muted-foreground">
                    当前后端：{searchBackend}
                  </span>
                </span>
                <span
                  className={cn(
                    "relative h-5 w-9 rounded-full transition-colors",
                    webSearch
                      ? "bg-[hsl(var(--status-web-search))]"
                      : "bg-muted",
                  )}
                  aria-hidden
                >
                  <span
                    className={cn(
                      "absolute top-0.5 size-4 rounded-full bg-white shadow-sm transition-transform",
                      webSearch ? "translate-x-4" : "translate-x-0.5",
                    )}
                  />
                </span>
              </button>
            </SettingCard>
          </div>

          <SettingCard
            title="系统状态"
            description="快速确认本地库、索引与模型连接是否处在可用状态。"
          >
            <div className="grid gap-2 sm:grid-cols-2">
              <StatusRow
                icon={HardDrive}
                label="Vault"
                value="由当前桌面会话管理"
                ready
              />
              <StatusRow
                icon={Database}
                label="索引"
                value="SQLite 派生索引"
                ready
              />
              <StatusRow
                icon={Sparkles}
                label="模型连接"
                value={status?.llm.message ?? "未检测"}
                ready={llmReady}
              />
              <StatusRow
                icon={Wifi}
                label="检索后端"
                value={searchBackend}
                ready={Boolean(status?.searchApi)}
              />
            </div>
          </SettingCard>

          <SettingCard
            title="数据与隐私"
            description="笔记正文以 Markdown 文件为准；运行状态与缓存保存在本机。"
          >
            <div className="grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
              <div className="rounded-md border border-border/45 bg-background/35 px-3 py-2">
                <div className="mb-1 flex items-center gap-1.5 font-medium text-foreground">
                  <ShieldCheck className="h-3.5 w-3.5" />
                  凭据保护
                </div>
                API Key 仅写入系统凭据管理器，不进入明文文件或日志。
              </div>
              <div className="rounded-md border border-border/45 bg-background/35 px-3 py-2">
                <div className="mb-1 flex items-center gap-1.5 font-medium text-foreground">
                  <Database className="h-3.5 w-3.5" />
                  本地索引
                </div>
                笔记索引可从 Markdown 重建，应用状态独立保存。
              </div>
            </div>
          </SettingCard>

          <section data-testid="settings-section-about">
            <h3 className="mb-2 text-xs font-medium text-foreground">
              关于 Iris
            </h3>
            <div className="rounded-lg border border-border/70 bg-surface-inset/35 px-3 py-2 text-xs leading-5 text-muted-foreground">
              <div className="font-medium text-foreground">Iris</div>
              <div>版本 1.1.0</div>
              <div>Copyright (C) 2026 Iris Contributors</div>
              <div>Licensed under GNU Affero General Public License v3.0</div>
              <div>
                License: <span className="font-mono">LICENSE</span>
                <span className="px-1 text-muted-foreground/60">·</span>
                Source:{" "}
                <span className="font-mono">
                  https://github.com/skahanium/iris
                </span>
              </div>
            </div>
          </section>
        </div>
      </ScrollArea>
    </IrisOverlay>
  );
}
