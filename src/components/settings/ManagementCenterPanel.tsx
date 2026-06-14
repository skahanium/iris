import {
  Bot,
  ChevronLeft,
  CheckCircle2,
  Clock3,
  Database,
  FileClock,
  Globe2,
  HardDrive,
  Info,
  KeyRound,
  Link2,
  Puzzle,
  ShieldCheck,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanelBody } from "@/components/ai/SkillsPanel";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { useConnectivityStatus } from "@/hooks/useConnectivityStatus";
import type { ManagementCenterSection } from "@/hooks/useOverlayManager";
import { cn } from "@/lib/utils";

import { LlmRoutingSection } from "./LlmRoutingSection";
import { MinimaxSearchSection } from "./MinimaxSearchSection";
import { PersonaSettingsBody } from "./PersonaSettingsPanel";

interface ManagementCenterPanelProps {
  open: boolean;
  onClose: () => void;
  section: ManagementCenterSection;
  webSearch: boolean;
  onWebSearchChange: (enabled: boolean) => void;
  onOpenKnowledgeRelations: () => void;
  onOpenVersion: () => void;
  onRescanVault: () => void;
  autoVersionEnabled: boolean;
  autoVersionIdleMinutes: number;
  onAutoVersionEnabledChange: (enabled: boolean) => void;
  onAutoVersionIdleMinutesChange: (minutes: number) => void;
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

const AI_DETAIL_META: Record<
  AiManagementDetail,
  { label: string; detail: string; icon: LucideIcon }
> = {
  models: {
    label: "模型与供应商",
    detail: "供应商凭据、诊断与能力槽模型路由。",
    icon: SlidersHorizontal,
  },
  "web-search": {
    label: "联网与证据",
    detail: "联网检索开关、后端与搜索配置。",
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

function SwitchControl({
  checked,
  onCheckedChange,
  label,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={cn(
        "relative h-6 w-11 rounded-full transition-colors",
        checked ? "bg-[hsl(var(--status-llm-ready))]" : "bg-muted",
      )}
      onClick={() => onCheckedChange(!checked)}
    >
      <span
        className={cn(
          "absolute top-1 size-4 rounded-full bg-white shadow-sm transition-transform",
          checked ? "translate-x-6" : "translate-x-1",
        )}
      />
    </button>
  );
}

export function ManagementCenterPanel({
  open,
  onClose,
  section,
  webSearch,
  onWebSearchChange,
  onOpenKnowledgeRelations,
  onOpenVersion,
  onRescanVault,
  autoVersionEnabled,
  autoVersionIdleMinutes,
  onAutoVersionEnabledChange,
  onAutoVersionIdleMinutesChange,
}: ManagementCenterPanelProps) {
  const [activeSection, setActiveSection] =
    useState<ManagementCenterSection>(section);
  const [activeAiDetail, setActiveAiDetail] =
    useState<AiManagementDetail | null>(null);
  const { status } = useConnectivityStatus();

  useEffect(() => {
    if (!open) return;
    setActiveSection(section);
    setActiveAiDetail(null);
  }, [open, section]);

  const activeMeta = useMemo(
    () =>
      MANAGEMENT_SECTIONS.find((item) => item.id === activeSection) ??
      MANAGEMENT_SECTIONS[0]!,
    [activeSection],
  );

  const searchBackend =
    status?.searchApi.effectiveBackend === "minimax"
      ? "MiniMax"
      : "DuckDuckGo / 本地备用";
  const llmReady = status?.llm.state === "ready";

  const openAiDetail = (detail: AiManagementDetail) => {
    setActiveSection("ai");
    setActiveAiDetail(detail);
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
          detail={webSearch ? `当前后端：${searchBackend}` : "联网检索已关闭。"}
        >
          <StatusValue ready={Boolean(webSearch && status?.searchApi)}>
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
          detail="API Key 保存在系统凭据管理器；不写入 Markdown、SQLite 正文或日志。"
        />
        <SettingRow
          icon={Info}
          title="关于 Iris"
          detail={
            <>
              版本 1.1.0 · GNU Affero General Public License v3.0 · Source:
              <span className="ml-1 font-mono">
                https://github.com/skahanium/iris
              </span>
            </>
          }
        />
      </PanelSection>
    </SectionShell>
  );

  const renderNotes = () => (
    <SectionShell title="笔记" detail="保存策略、版本安全网和手动恢复入口。">
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
            <PanelSection title="联网状态">
              <SettingRow
                icon={Globe2}
                title={webSearch ? "联网已开启" : "联网已关闭"}
                detail={`当前后端：${searchBackend}`}
              >
                <SwitchControl
                  checked={webSearch}
                  label="联网检索"
                  onCheckedChange={onWebSearchChange}
                />
              </SettingRow>
            </PanelSection>
            <MinimaxSearchSection open={open} />
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
