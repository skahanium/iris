import {
  ArchiveRestore,
  AlertCircle,
  Bot,
  ChevronLeft,
  CheckCircle2,
  Clock3,
  Cpu,
  Database,
  DownloadCloud,
  FileClock,
  FolderTree,
  Globe2,
  HardDrive,
  Info,
  KeyRound,
  Link2,
  Puzzle,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanelBody } from "@/components/ai/SkillsPanel";
import { McpProfilesPanel } from "@/components/ai/skills/McpProfilesPanel";
import { RecycleBinBody } from "@/components/file/RecycleBinSheet";
import { VaultNavigatorBody } from "@/components/file/VaultNavigator";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import type {
  ManagementCenterDetail,
  ManagementCenterSection,
} from "@/hooks/useOverlayManager";
import type { AppUpdateSnapshot } from "@/hooks/useAppUpdate";
import { isWindowsDesktopChrome } from "@/lib/platform-chrome";
import { cn } from "@/lib/utils";
import {
  webSearchStatusDetail,
  type WebSearchAvailability,
  type WebSearchProviderOption,
} from "@/lib/web-search-provider-state";
import type {
  EmbeddingIndexStatus,
  EmbeddingSchedulerStartResult,
  FileListItem,
} from "@/types/ipc";
import type { ConnectivityStatus } from "@/types/llm";

import { LlmRoutingSection } from "./LlmRoutingSection";
import { PersonaSettingsBody } from "./PersonaSettingsPanel";

interface ManagementCenterPanelProps {
  open: boolean;
  onClose: () => void;
  section: ManagementCenterSection;
  detail: ManagementCenterDetail;
  webSearch: boolean;
  webSearchAvailability: WebSearchAvailability;
  webSearchProviderId: string | null;
  webSearchProviders: WebSearchProviderOption[];
  connectivityStatus: ConnectivityStatus | null;
  onWebSearchChange: (enabled: boolean) => void;
  onWebSearchProviderChange: (providerId: string | null) => void;
  onRefreshWebSearchProviders: () => Promise<void>;
  onOpenNote: (path: string) => void | Promise<void>;
  onPrepareNote?: (file: FileListItem) => void;
  onOpenKnowledgeRelations: () => void;
  onOpenVersion: () => void;
  onRescanVault: () => void;
  onRecycleIndexChange: () => void;
  onIndexDegraded: () => void;
  onBeforeFilePathChange?: (oldPath: string, newPath: string) => Promise<void>;
  onFilePathChanged?: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  onFilePathChangeFailed?: (oldPath: string) => void;
  onBeforeFileDelete?: (path: string) => Promise<void>;
  onFileDeleted?: (path: string) => void;
  onIndexChange?: () => void;
  autoVersionEnabled: boolean;
  autoVersionIdleMinutes: number;
  onAutoVersionEnabledChange: (enabled: boolean) => void;
  onAutoVersionIdleMinutesChange: (minutes: number) => void;
  embeddingStatus: EmbeddingIndexStatus | null;
  embeddingStatusLoading: boolean;
  onSetEmbeddingPaused: (paused: boolean) => Promise<void>;
  onStartEmbeddingRebuild: () => Promise<EmbeddingSchedulerStartResult | null>;
  appUpdate: AppUpdateSnapshot;
  hasDirtyDocuments: boolean;
  onCheckUpdate: () => Promise<void>;
  onDownloadUpdate: () => Promise<void>;
  onInstallUpdate: () => Promise<void>;
}

interface ManagementSectionMeta {
  id: ManagementCenterSection;
  label: string;
  detail: string;
}

const MANAGEMENT_SECTIONS: ManagementSectionMeta[] = [
  { id: "overview", label: "总览", detail: "状态仪表" },
  { id: "notes", label: "笔记", detail: "保存与版本" },
  { id: "knowledge", label: "知识库", detail: "索引维护" },
  { id: "ai", label: "AI", detail: "模型与工具" },
];

type AiManagementDetail =
  | "models"
  | "web-search"
  | "persona"
  | "skills"
  | "memory";

type NotesManagementDetail = "file-sheet" | "recycle-bin";

const WINDOWS_INSTALL_WARNING = "Windows 上安装会关闭 Iris。";

const AI_DETAIL_META: Record<
  AiManagementDetail,
  { label: string; detail: string; icon: LucideIcon }
> = {
  models: {
    label: "模型与供应商",
    detail: "供应商凭据、模型验证与默认模型池。",
    icon: SlidersHorizontal,
  },
  "web-search": {
    label: "联网与证据",
    detail: "联网检索开关、MCP 提供方与证据配置。",
    icon: Globe2,
  },
  persona: {
    label: "人格与写作风格",
    detail: "称呼、头像、表达风格与常驻规则。",
    icon: Bot,
  },
  skills: {
    label: "Skills 与工具",
    detail: "安装、启停、编辑和诊断 Skills。",
    icon: Puzzle,
  },
  memory: {
    label: "记忆与规则",
    detail: "查看、禁用和删除 AI 规则。",
    icon: FileClock,
  },
};

function SectionShell({
  title,
  detail,
  children,
}: {
  title: string;
  detail: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-5">
      <header className="border-b border-border/60 pb-3">
        <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
      </header>
      {children}
    </section>
  );
}

function PanelSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-2">
      <h4 className="text-xs font-semibold text-muted-foreground">{title}</h4>
      <div className="overflow-hidden rounded-lg border border-border/65 bg-background/55">
        {children}
      </div>
    </div>
  );
}

function SettingRow({
  icon: Icon,
  title,
  detail,
  children,
}: {
  icon?: LucideIcon;
  title: string;
  detail?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="grid gap-3 border-b border-border/50 px-4 py-3 last:border-b-0 md:grid-cols-[minmax(12rem,1fr)_auto] md:items-center">
      <div className="flex min-w-0 gap-3">
        {Icon ? (
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface-inset text-muted-foreground">
            <Icon className="h-4 w-4" />
          </span>
        ) : null}
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">{title}</p>
          {detail ? (
            <div className="mt-1 text-xs leading-relaxed text-muted-foreground">
              {detail}
            </div>
          ) : null}
        </div>
      </div>
      {children ? (
        <div className="flex items-center gap-2">{children}</div>
      ) : null}
    </div>
  );
}

function StatusValue({
  ready,
  children,
}: {
  ready?: boolean;
  children: ReactNode;
}) {
  return (
    <span className="inline-flex items-center gap-2 rounded-md border border-border/50 bg-surface-inset/45 px-2.5 py-1 text-xs text-foreground">
      {typeof ready === "boolean" ? (
        <span
          className={cn(
            "size-2 rounded-full",
            ready
              ? "bg-[hsl(var(--status-llm-ready))]"
              : "bg-[hsl(var(--status-inactive)/0.65)]",
          )}
          aria-hidden
        />
      ) : null}
      {children}
    </span>
  );
}

function updateStatusText(status: AppUpdateSnapshot["status"]) {
  switch (status) {
    case "checking":
      return "正在检查更新";
    case "up_to_date":
      return "已是最新版";
    case "available":
      return "发现新版本";
    case "downloading":
      return "正在下载更新";
    case "downloaded":
      return "更新已下载";
    case "ready_to_install":
      return "可以安装";
    case "unsupported":
      return "当前平台暂不支持应用内更新";
    case "error":
      return "无法检查更新";
    default:
      return "尚未检查";
  }
}

function updateProgressText(appUpdate: AppUpdateSnapshot) {
  const progress = appUpdate.progress;
  if (!progress || appUpdate.status !== "downloading") return null;
  if (!progress.contentLength) {
    return `已下载 ${(progress.downloaded / 1024 / 1024).toFixed(1)} MB`;
  }
  const percent = Math.min(
    100,
    Math.round((progress.downloaded / progress.contentLength) * 100),
  );
  return `${percent}% · ${(progress.downloaded / 1024 / 1024).toFixed(1)} MB / ${(
    progress.contentLength /
    1024 /
    1024
  ).toFixed(1)} MB`;
}

function appUpdateMessageText(message: string) {
  return message.includes("signature") ? "更新包验证失败" : message;
}

function SwitchControl({
  checked,
  onCheckedChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={cn(
        "relative inline-flex h-7 w-12 shrink-0 overflow-hidden rounded-full border p-0 transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        checked
          ? "border-[hsl(var(--status-llm-ready)/0.72)] bg-[hsl(var(--status-llm-ready))] shadow-[inset_0_1px_0_hsl(0_0%_100%/0.20),0_0_0_1px_hsl(var(--status-llm-ready)/0.12)]"
          : "border-border/70 bg-surface-inset shadow-inner",
        disabled && "cursor-not-allowed opacity-55",
      )}
      onClick={() => {
        if (!disabled) onCheckedChange(!checked);
      }}
    >
      <span
        className={cn(
          "pointer-events-none absolute left-1 top-1 size-5 rounded-full bg-white shadow-[0_1px_2px_hsl(0_0%_0%/0.24),0_0_0_1px_hsl(0_0%_0%/0.06)] ring-1 ring-black/5 transition-transform duration-200 ease-out",
          checked ? "translate-x-5" : "translate-x-0",
        )}
      />
    </button>
  );
}
function isAiManagementDetail(
  detail: ManagementCenterDetail,
): detail is AiManagementDetail {
  return (
    detail === "models" ||
    detail === "web-search" ||
    detail === "persona" ||
    detail === "skills" ||
    detail === "memory"
  );
}

function isNotesManagementDetail(
  detail: ManagementCenterDetail,
): detail is NotesManagementDetail {
  return detail === "file-sheet" || detail === "recycle-bin";
}

function embeddingFailureDetail(
  failureCode: EmbeddingIndexStatus["failureCode"],
): string {
  switch (failureCode) {
    case "model_unavailable":
      return "模型不可用，可稍后手动重试。";
    case "interrupted_migration":
    case "interrupted_restart":
      return "上一次后台重建已中断，可手动重试。";
    case "database_error":
      return "索引数据库暂不可用，可稍后手动重试。";
    case "embedding_failed":
      return "嵌入生成未完成，可手动重试。";
    default:
      return "后台重建未完成，可手动重试。";
  }
}

export function ManagementCenterPanel({
  open,
  onClose,
  section,
  detail,
  webSearch,
  webSearchAvailability,
  webSearchProviderId,
  webSearchProviders,
  connectivityStatus,
  onWebSearchChange,
  onWebSearchProviderChange,
  onRefreshWebSearchProviders,
  onOpenNote,
  onPrepareNote,
  onOpenKnowledgeRelations,
  onOpenVersion,
  onRescanVault,
  onRecycleIndexChange,
  onIndexDegraded,
  onBeforeFilePathChange,
  onFilePathChanged,
  onFilePathChangeFailed,
  onBeforeFileDelete,
  onFileDeleted,
  onIndexChange,
  autoVersionEnabled,
  autoVersionIdleMinutes,
  onAutoVersionEnabledChange,
  onAutoVersionIdleMinutesChange,
  embeddingStatus,
  embeddingStatusLoading,
  onSetEmbeddingPaused,
  onStartEmbeddingRebuild,
  appUpdate,
  hasDirtyDocuments,
  onCheckUpdate,
  onDownloadUpdate,
  onInstallUpdate,
}: ManagementCenterPanelProps) {
  const [activeSection, setActiveSection] =
    useState<ManagementCenterSection>(section);
  const [activeAiDetail, setActiveAiDetail] =
    useState<AiManagementDetail | null>(null);
  const [activeNotesDetail, setActiveNotesDetail] =
    useState<NotesManagementDetail | null>(null);
  const status = connectivityStatus;
  const [embeddingActionMessage, setEmbeddingActionMessage] = useState<
    string | null
  >(null);

  const handleEmbeddingStart = () => {
    setEmbeddingActionMessage(null);
    void onStartEmbeddingRebuild()
      .then((result) => {
        if (result === null) {
          setEmbeddingActionMessage("无法启动后台重建，请稍后手动重试");
        }
      })
      .catch(() => {
        setEmbeddingActionMessage("无法启动后台重建，请稍后手动重试");
      });
  };

  const handleEmbeddingPaused = (paused: boolean) => {
    setEmbeddingActionMessage(null);
    void onSetEmbeddingPaused(paused).catch(() => {
      setEmbeddingActionMessage("无法更新后台重建状态，请稍后手动重试");
    });
  };

  useEffect(() => {
    if (!open) return;
    setActiveSection(section);
    setActiveAiDetail(
      section === "ai" && isAiManagementDetail(detail) ? detail : null,
    );
    setActiveNotesDetail(
      section === "notes" && isNotesManagementDetail(detail) ? detail : null,
    );
  }, [detail, open, section]);

  const activeMeta = useMemo(
    () =>
      MANAGEMENT_SECTIONS.find((item) => item.id === activeSection) ??
      MANAGEMENT_SECTIONS[0]!,
    [activeSection],
  );

  const searchBackend =
    webSearchAvailability.effectiveProvider?.name ??
    webSearchAvailability.detail;
  const searchBackendReady = webSearchAvailability.canEnable;
  const webSearchDetail = webSearchStatusDetail(
    webSearch,
    webSearchAvailability,
  );
  const selectedWebSearchProviderId =
    webSearchProviderId ??
    (webSearchAvailability.options.length === 1
      ? (webSearchAvailability.options[0]?.id ?? "")
      : "");
  const configuredWebSearchProviderCount = webSearchProviders.filter(
    (provider) => provider.providerKind === "mcp" && provider.hasSearchMapping,
  ).length;
  const llmReady = status?.llm.state === "ready";
  const updateProgress = updateProgressText(appUpdate);
  const failedPreflightChecks =
    appUpdate.preflight?.checks.filter((check) => check.status === "failed") ??
    [];
  const showWindowsInstallWarning =
    isWindowsDesktopChrome() && appUpdate.status === "ready_to_install";

  const openAiDetail = (detail: AiManagementDetail) => {
    setActiveSection("ai");
    setActiveAiDetail(detail);
    setActiveNotesDetail(null);
  };

  const openNotesDetail = (nextDetail: NotesManagementDetail) => {
    setActiveSection("notes");
    setActiveNotesDetail(nextDetail);
    setActiveAiDetail(null);
  };

  const renderOverview = () => (
    <SectionShell title="总览" detail="Iris 的当前运行状态和系统边界。">
      <PanelSection title="状态">
        <SettingRow
          icon={HardDrive}
          title="Vault"
          detail="普通 Markdown 笔记库由桌面会话管理。"
        >
          <StatusValue ready>已连接</StatusValue>
        </SettingRow>
        <SettingRow
          icon={Bot}
          title="当前模型"
          detail={status?.llm.message ?? "尚未从后端读取模型状态。"}
        >
          <StatusValue ready={llmReady}>
            {llmReady ? "可用" : "需检查"}
          </StatusValue>
        </SettingRow>
        <SettingRow
          icon={Globe2}
          title="联网"
          detail={webSearch ? `联网证据：${searchBackend}` : "联网检索已关闭。"}
        >
          <StatusValue ready={Boolean(webSearch && searchBackendReady)}>
            {webSearch ? "开启" : "关闭"}
          </StatusValue>
        </SettingRow>
        <SettingRow
          icon={Database}
          title="索引"
          detail="SQLite 保存派生索引；Markdown 仍是笔记知识的权威来源。"
        >
          <StatusValue ready>派生索引</StatusValue>
        </SettingRow>
        <SettingRow
          icon={FileClock}
          title="自动版本"
          detail={`空闲 ${autoVersionIdleMinutes} 分钟后生成低噪自动备份。`}
        >
          <StatusValue ready={autoVersionEnabled}>
            {autoVersionEnabled ? "开启" : "关闭"}
          </StatusValue>
        </SettingRow>
      </PanelSection>

      <PanelSection title="系统边界">
        <SettingRow
          icon={ShieldCheck}
          title="权限边界"
          detail="写入笔记、网页抓取和外部动作执行前需要明确确认。"
        />
        <SettingRow
          icon={KeyRound}
          title="凭据边界"
          detail="API Key 保存在本地加密凭据；不写入 Markdown、SQLite 正文或日志。"
        />
        <SettingRow
          icon={Info}
          title="关于 Iris"
          detail={
            <>
              版本 1.2.9 · GNU Affero General Public License v3.0 · Source:
              <span className="ml-1 font-mono">
                https://github.com/skahanium/iris
              </span>
            </>
          }
        />
        <SettingRow
          icon={DownloadCloud}
          title="应用更新"
          detail={
            <div className="space-y-2">
              <p>
                {updateStatusText(appUpdate.status)}
                {appUpdate.info ? (
                  <>
                    {" "}
                    · 当前 {appUpdate.info.currentVersion} · 新版本{" "}
                    {appUpdate.info.version}
                  </>
                ) : null}
              </p>
              {appUpdate.info?.pubDate ? (
                <p>发布时间：{appUpdate.info.pubDate}</p>
              ) : null}
              {appUpdate.info?.notes ? (
                <p className="line-clamp-3 whitespace-pre-line">
                  {appUpdate.info.notes}
                </p>
              ) : null}
              {updateProgress ? <p>{updateProgress}</p> : null}
              {appUpdate.message ? (
                <p className="text-destructive">
                  {appUpdateMessageText(appUpdate.message)}
                </p>
              ) : null}
              {appUpdate.status === "unsupported" ? (
                <p>
                  可打开{" "}
                  <a
                    className="text-foreground underline underline-offset-4"
                    href="https://github.com/skahanium/iris/releases"
                    target="_blank"
                    rel="noreferrer"
                  >
                    GitHub Release
                  </a>{" "}
                  手动下载。
                </p>
              ) : null}
              {failedPreflightChecks.length > 0 ? (
                <div className="space-y-1 text-destructive">
                  {failedPreflightChecks.map((check) => (
                    <p key={check.id}>
                      {check.label}：{check.message}
                    </p>
                  ))}
                </div>
              ) : null}
              {appUpdate.preflight?.ok ? (
                <p className="text-foreground">兼容性预检已通过。</p>
              ) : null}
              {hasDirtyDocuments && appUpdate.status === "ready_to_install" ? (
                <p className="text-destructive">
                  安装时将先保存所有未确认落盘的 Markdown。
                </p>
              ) : null}
              {appUpdate.installBlockedMessage ? (
                <p className="text-destructive">
                  {appUpdate.installBlockedMessage}
                </p>
              ) : null}
              {showWindowsInstallWarning ? (
                <p>{WINDOWS_INSTALL_WARNING}</p>
              ) : null}
              <p className="sr-only">更新包验证失败</p>
            </div>
          }
        >
          <Button
            size="sm"
            variant="outline"
            disabled={appUpdate.busy}
            onClick={() => void onCheckUpdate()}
          >
            检查更新
          </Button>
          {appUpdate.status === "available" ? (
            <Button
              size="sm"
              variant="outline"
              disabled={appUpdate.busy}
              onClick={() => void onDownloadUpdate()}
            >
              下载更新
            </Button>
          ) : null}
          {appUpdate.status === "ready_to_install" ? (
            <Button
              size="sm"
              variant="outline"
              disabled={appUpdate.busy}
              onClick={() => void onInstallUpdate()}
            >
              安装并重启
            </Button>
          ) : null}
          {failedPreflightChecks.length > 0 ? (
            <StatusValue>
              <AlertCircle className="h-3.5 w-3.5" />
              预检失败
            </StatusValue>
          ) : null}
        </SettingRow>
      </PanelSection>
    </SectionShell>
  );

  const renderNotesOverview = () => (
    <SectionShell title="笔记" detail="保存策略、版本安全网和手动恢复入口。">
      <PanelSection title="笔记库管理">
        <SettingRow
          icon={FolderTree}
          title="浏览笔记库"
          detail="以文件树方式浏览、创建、移动、重命名、锁定和删除 Markdown 笔记。"
        >
          <StatusValue>Ctrl/Cmd+Shift+E</StatusValue>
          <Button
            size="sm"
            variant="outline"
            onClick={() => openNotesDetail("file-sheet")}
          >
            打开
          </Button>
        </SettingRow>
        <SettingRow
          icon={ArchiveRestore}
          title="回收站"
          detail="恢复最近删除的笔记、时间线快照与定稿版本，或执行永久删除。"
        >
          <Button
            size="sm"
            variant="outline"
            onClick={() => openNotesDetail("recycle-bin")}
          >
            打开
          </Button>
        </SettingRow>
      </PanelSection>

      <PanelSection title="版本追踪">
        <SettingRow
          icon={FileClock}
          title="版本面板"
          detail="Cmd/Ctrl+Shift+V 打开版本追踪；定稿、检查点和恢复都在面板内完成。"
        >
          <Button size="sm" variant="outline" onClick={onOpenVersion}>
            打开版本
          </Button>
        </SettingRow>
        <SettingRow
          icon={Clock3}
          title="自动版本追踪"
          detail="开启后，空闲达到设定时间会保存自动备份；版本面板内折叠为自动备份（N）。"
        >
          <SwitchControl
            checked={autoVersionEnabled}
            label="自动版本追踪"
            onCheckedChange={onAutoVersionEnabledChange}
          />
        </SettingRow>
        <SettingRow
          title="空闲间隔"
          detail="取值范围 1-120 分钟，默认 10 分钟。"
        >
          <input
            type="number"
            min={1}
            max={120}
            value={autoVersionIdleMinutes}
            className="h-8 w-24 rounded-md border border-border bg-background px-2 text-sm text-foreground"
            onChange={(event) =>
              onAutoVersionIdleMinutesChange(Number(event.target.value))
            }
          />
          <span className="text-xs text-muted-foreground">分钟</span>
        </SettingRow>
      </PanelSection>

      <PanelSection title="保存策略">
        <SettingRow
          icon={CheckCircle2}
          title="直接保存"
          detail="Cmd/Ctrl+S 只保存当前 Markdown 内容；自动版本追踪是额外安全网。"
        />
      </PanelSection>
    </SectionShell>
  );

  const renderNotesDetail = (notesDetail: NotesManagementDetail) => {
    const isFileTree = notesDetail === "file-sheet";
    return (
      <section className="flex min-h-[34rem] flex-col">
        <header className="flex items-start gap-3 border-b border-border/60 pb-3">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid="management-detail-back"
            className="h-8 gap-1.5"
            onClick={() => setActiveNotesDetail(null)}
          >
            <ChevronLeft className="h-4 w-4" />
            笔记
          </Button>
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-foreground">
              {isFileTree ? "浏览笔记库" : "回收站"}
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">
              {isFileTree
                ? "文件树、文档列表和文件操作在同一个管理中心面板内完成。"
                : "已删除笔记保留 15 天，恢复后会回到原路径。"}
            </p>
          </div>
        </header>

        <div className="mt-4 flex h-[min(58vh,34rem)] min-h-[28rem] flex-col overflow-hidden rounded-lg border border-border/65 bg-background/55">
          {isFileTree ? (
            <VaultNavigatorBody
              open={open && activeSection === "notes" && isFileTree}
              onClose={onClose}
              onOpen={onOpenNote}
              onPrepare={onPrepareNote}
              onBeforeFilePathChange={onBeforeFilePathChange}
              onFilePathChanged={onFilePathChanged}
              onFilePathChangeFailed={onFilePathChangeFailed}
              onBeforeFileDelete={onBeforeFileDelete}
              onFileDeleted={onFileDeleted}
              onIndexDegraded={onIndexDegraded}
              onIndexChange={onIndexChange}
            />
          ) : (
            <RecycleBinBody
              open={open && activeSection === "notes" && !isFileTree}
              onClose={onClose}
              onRestored={onOpenNote}
              onIndexDegraded={onIndexDegraded}
              onIndexChange={onRecycleIndexChange}
            />
          )}
        </div>
      </section>
    );
  };

  const renderNotes = () =>
    activeNotesDetail
      ? renderNotesDetail(activeNotesDetail)
      : renderNotesOverview();

  const renderKnowledge = () => (
    <SectionShell title="知识库" detail="只管理索引维护和当前笔记知识关联。">
      <PanelSection title="索引维护">
        <SettingRow
          icon={Database}
          title="派生索引"
          detail="外部批量修改 Markdown 后，可手动从权威文件重建索引。"
        >
          <Button size="sm" variant="outline" onClick={onRescanVault}>
            重建库索引
          </Button>
        </SettingRow>
        <SettingRow
          icon={Link2}
          title="知识关联"
          detail="反向链接与标签在同一个任务舱中切换，用于当前笔记上下文追踪。"
        >
          <Button
            size="sm"
            variant="outline"
            onClick={onOpenKnowledgeRelations}
          >
            打开关联
          </Button>
        </SettingRow>
      </PanelSection>

      <PanelSection title="搜索与嵌入">
        <SettingRow
          icon={Cpu}
          title="嵌入模型"
          detail={
            embeddingStatus
              ? `${embeddingStatus.targetModelId} · ${embeddingStatus.dimension} 维 · ${embeddingStatus.indexedItems}/${embeddingStatus.totalItems} 项`
              : embeddingStatusLoading
                ? "正在读取嵌入索引状态…"
                : "暂时无法读取嵌入索引状态。"
          }
        >
          <StatusValue ready={embeddingStatus?.phase === "ready"}>
            {embeddingStatus?.phase === "ready"
              ? "就绪"
              : embeddingStatus?.phase === "running"
                ? "后台重建"
                : embeddingStatus?.phase === "paused"
                  ? "已暂停"
                  : embeddingStatus?.phase === "failed"
                    ? "失败但不影响编辑"
                    : embeddingStatus?.phase === "legacy_ready"
                      ? "旧版检索可用，等待空闲升级"
                      : "未知"}
          </StatusValue>
        </SettingRow>
        {embeddingStatus?.phase === "running" ? (
          <SettingRow
            icon={RefreshCw}
            title="嵌入重建"
            detail="后台正在低优先级重建；继续编辑会在批次边界暂停。"
          >
            <Button
              size="sm"
              variant="outline"
              onClick={() => handleEmbeddingPaused(true)}
            >
              暂停
            </Button>
          </SettingRow>
        ) : embeddingStatus?.phase === "paused" ? (
          <SettingRow
            icon={RefreshCw}
            title="嵌入重建"
            detail="后台重建已暂停，保留当前进度。"
          >
            <Button
              size="sm"
              variant="outline"
              onClick={() => handleEmbeddingPaused(false)}
            >
              继续
            </Button>
          </SettingRow>
        ) : embeddingStatus?.phase === "failed" ? (
          <SettingRow
            icon={AlertCircle}
            title="嵌入重建"
            detail={embeddingFailureDetail(embeddingStatus.failureCode)}
          >
            <Button size="sm" variant="outline" onClick={handleEmbeddingStart}>
              手动重试
            </Button>
          </SettingRow>
        ) : (
          <SettingRow
            icon={RefreshCw}
            title="嵌入重建"
            detail="后台重建不影响 Markdown 编辑或保存。"
          >
            <Button size="sm" variant="outline" onClick={handleEmbeddingStart}>
              后台重建
            </Button>
          </SettingRow>
        )}
        {embeddingActionMessage ? (
          <p
            className="px-3 text-xs text-destructive"
            role="status"
            aria-live="polite"
          >
            {embeddingActionMessage}
          </p>
        ) : null}
      </PanelSection>

      <PanelSection title="边界">
        <SettingRow
          icon={HardDrive}
          title="数据来源"
          detail="用户 .md 文件是笔记知识的权威来源；索引可以重建，不替代笔记正文。"
        />
      </PanelSection>
    </SectionShell>
  );

  const renderAiOverview = () => (
    <SectionShell
      title="AI"
      detail="复杂配置可以进入 AI 内部子页；顶层只保留清晰入口。"
    >
      <div className="space-y-5" data-testid="management-section-ai">
        <PanelSection title="配置">
          {(
            Object.entries(AI_DETAIL_META) as Array<
              [AiManagementDetail, (typeof AI_DETAIL_META)[AiManagementDetail]]
            >
          ).map(([id, item]) => (
            <SettingRow
              key={id}
              icon={item.icon}
              title={item.label}
              detail={item.detail}
            >
              <Button
                size="sm"
                variant="outline"
                onClick={() => openAiDetail(id)}
              >
                打开
              </Button>
            </SettingRow>
          ))}
        </PanelSection>
      </div>
    </SectionShell>
  );

  const renderAiDetail = (detail: AiManagementDetail) => {
    const meta = AI_DETAIL_META[detail];
    return (
      <section data-testid="management-ai-detail" className="space-y-5">
        <header className="flex items-start gap-3 border-b border-border/60 pb-3">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            data-testid="management-detail-back"
            className="h-8 gap-1.5"
            onClick={() => setActiveAiDetail(null)}
          >
            <ChevronLeft className="h-4 w-4" />
            AI
          </Button>
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-foreground">
              {meta.label}
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">{meta.detail}</p>
          </div>
        </header>

        {detail === "models" ? <LlmRoutingSection open={open} /> : null}
        {detail === "web-search" ? (
          <div className="space-y-5">
            <PanelSection title="联网搜索">
              <SettingRow
                icon={Globe2}
                title="当前搜索提供方"
                detail={
                  webSearchAvailability.canEnable
                    ? `将使用 ${searchBackend} 执行联网搜索。`
                    : `${webSearchAvailability.detail}。已配置 ${configuredWebSearchProviderCount} 个 MCP 搜索映射。`
                }
              >
                <select
                  value={selectedWebSearchProviderId}
                  disabled={webSearchAvailability.options.length === 0}
                  className="h-8 min-w-40 rounded-md border border-border bg-background px-2 text-xs text-foreground disabled:cursor-not-allowed disabled:opacity-55"
                  onChange={(event) =>
                    onWebSearchProviderChange(event.target.value || null)
                  }
                >
                  <option value="">请选择</option>
                  {webSearchAvailability.options.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name}
                    </option>
                  ))}
                </select>
              </SettingRow>
              <SettingRow
                icon={Globe2}
                title={webSearch ? "联网已开启" : "联网已关闭"}
                detail={`联网：${webSearchDetail}`}
              >
                <SwitchControl
                  checked={webSearch && webSearchAvailability.canEnable}
                  disabled={!webSearchAvailability.canEnable}
                  label="联网搜索"
                  onCheckedChange={(checked) => {
                    if (!checked || webSearchAvailability.canEnable) {
                      onWebSearchChange(checked);
                    }
                  }}
                />
              </SettingRow>
            </PanelSection>
            <McpProfilesPanel
              open={open}
              onProvidersChanged={onRefreshWebSearchProviders}
            />
          </div>
        ) : null}
        {detail === "persona" ? <PersonaSettingsBody open={open} /> : null}
        {detail === "skills" ? <SkillsPanelBody open={open} /> : null}
        {detail === "memory" ? <AiRulesPanel compact /> : null}
      </section>
    );
  };

  const renderAi = () =>
    activeAiDetail ? renderAiDetail(activeAiDetail) : renderAiOverview();

  const renderContent = () => {
    if (activeSection === "notes") return renderNotes();
    if (activeSection === "knowledge") return renderKnowledge();
    if (activeSection === "ai") return renderAi();
    return renderOverview();
  };

  return (
    <IrisOverlay
      open={open}
      onClose={onClose}
      title="管理中心"
      size="management"
      bodyClassName="overflow-hidden"
    >
      <div
        data-testid="management-center"
        className="flex min-h-0 flex-1 flex-col"
      >
        <div
          data-testid="management-center-tabs"
          className="grid w-full shrink-0 grid-cols-4 gap-2 border-b border-border/60 bg-surface-inset/20 px-4 py-3"
          role="tablist"
          aria-label="管理中心"
        >
          {MANAGEMENT_SECTIONS.map((item) => {
            const active = item.id === activeMeta.id;
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={active}
                className={cn(
                  "min-w-0 rounded-md px-5 py-4 text-left transition-colors",
                  active
                    ? "bg-task-selected text-foreground"
                    : "text-muted-foreground hover:bg-surface-inset hover:text-foreground",
                )}
                onClick={() => {
                  setActiveSection(item.id);
                  setActiveAiDetail(null);
                  setActiveNotesDetail(null);
                }}
              >
                <span className="block text-sm font-semibold">
                  {item.label}
                </span>
                <span className="mt-1 block text-xs opacity-75">
                  {item.detail}
                </span>
              </button>
            );
          })}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          {renderContent()}
        </div>
      </div>
    </IrisOverlay>
  );
}
