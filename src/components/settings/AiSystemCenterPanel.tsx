import { useState } from "react";

import {
  ArrowRight,
  Bot,
  ClipboardCheck,
  Puzzle,
  ShieldCheck,
  Sparkles,
  Wrench,
} from "lucide-react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

interface AiSystemCenterPanelProps {
  open: boolean;
  onClose: () => void;
}

type AiSystemSection = "models" | "search" | "persona" | "memory";

const AI_SYSTEM_SECTIONS: {
  id: AiSystemSection;
  label: string;
  detail: string;
}[] = [
  { id: "models", label: "模型路由", detail: "对话、写作、研究、Embedding" },
  { id: "search", label: "联网搜索", detail: "搜索后端、模型与可用性" },
  { id: "persona", label: "人格与 Skills", detail: "协作人格、工具能力与注入" },
  { id: "memory", label: "记忆与规则", detail: "用户规则、AI 记忆与确认" },
];

function SummaryTile({
  icon: Icon,
  label,
  value,
  detail,
}: {
  icon: typeof Sparkles;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/25 px-3 py-3">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Icon className="h-3.5 w-3.5" />
        <span className="text-[11px] font-medium">{label}</span>
      </div>
      <p className="mt-2 text-sm font-semibold text-foreground">{value}</p>
      <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
        {detail}
      </p>
    </div>
  );
}

function ActionCard({
  icon: Icon,
  title,
  detail,
  action,
  onClick,
}: {
  icon: typeof Sparkles;
  title: string;
  detail: string;
  action: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="ai-system-action-card"
      className="group flex min-h-[132px] w-full flex-col justify-between rounded-md border border-border/70 bg-background px-4 py-3 text-left transition-colors hover:border-primary/35 hover:bg-surface-inset/35"
      onClick={onClick}
    >
      <span className="flex items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border/60 bg-surface-inset text-muted-foreground group-hover:text-primary">
          <Icon className="h-4 w-4" />
        </span>
        <span className="min-w-0">
          <span className="block text-sm font-semibold text-foreground">
            {title}
          </span>
          <span className="mt-1 block text-xs leading-relaxed text-muted-foreground">
            {detail}
          </span>
        </span>
      </span>
      <span className="mt-4 inline-flex items-center gap-1 text-xs font-medium text-primary">
        {action}
        <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
      </span>
    </button>
  );
}

export function AiSystemCenterPanel({
  open,
  onClose,
}: AiSystemCenterPanelProps) {
  const [activeSection, setActiveSection] = useState<AiSystemSection>("models");
  const [personaOpen, setPersonaOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);
  const activeMeta =
    AI_SYSTEM_SECTIONS.find((section) => section.id === activeSection) ??
    AI_SYSTEM_SECTIONS[0]!;

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title="AI 系统中心"
        size="wide"
      >
        <div data-testid="ai-system-center" className="flex min-h-0 flex-1">
          <aside
            data-testid="ai-system-center-nav"
            className="w-56 shrink-0 border-r border-border/60 bg-surface-inset/20 p-2"
          >
            <nav className="space-y-1" aria-label="AI 系统中心">
              {AI_SYSTEM_SECTIONS.map((section) => {
                const active = section.id === activeSection;
                return (
                  <button
                    key={section.id}
                    type="button"
                    className={cn(
                      "w-full rounded-md px-3 py-2 text-left transition-colors duration-base ease-iris-out",
                      active
                        ? "bg-task-selected text-foreground"
                        : "text-muted-foreground hover:bg-surface-inset/70 hover:text-foreground",
                    )}
                    aria-current={active ? "page" : undefined}
                    onClick={() => setActiveSection(section.id)}
                  >
                    <span className="block text-xs font-medium">
                      {section.label}
                    </span>
                    <span className="mt-0.5 block truncate text-[11px] opacity-75">
                      {section.detail}
                    </span>
                  </button>
                );
              })}
            </nav>
          </aside>
          <ScrollArea className="min-h-0 flex-1">
            <div className="px-5 py-4">
              <header className="mb-4 border-b border-border/60 pb-3">
                <h3 className="text-sm font-medium text-foreground">
                  {activeMeta.label}
                </h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  {activeMeta.detail}
                </p>
              </header>

              {activeSection === "models" ? (
                <section>
                  <LlmRoutingSection open={open} />
                </section>
              ) : null}

              {activeSection === "search" ? (
                <section>
                  <MinimaxSearchSection open={open} />
                </section>
              ) : null}

              {activeSection === "persona" ? (
                <section
                  className="max-w-5xl space-y-4"
                  data-testid="ai-system-persona-dashboard"
                >
                  <div
                    className="grid gap-3 lg:grid-cols-3"
                    data-testid="ai-system-summary-grid"
                  >
                    <SummaryTile
                      icon={Bot}
                      label="人格"
                      value="对话身份与写作偏好"
                      detail="昵称、头像、语气和自定义规则会进入 AI 协作上下文。"
                    />
                    <SummaryTile
                      icon={Puzzle}
                      label="Skills"
                      value="场景化工具能力"
                      detail="按全局或当前库安装，匹配场景后再注入给助手。"
                    />
                    <SummaryTile
                      icon={ShieldCheck}
                      label="边界"
                      value="写入与高风险工具需确认"
                      detail="配置能力不等于自动执行，敏感操作仍走确认链路。"
                    />
                  </div>

                  <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(280px,0.78fr)]">
                    <div className="grid gap-3 md:grid-cols-2">
                      <ActionCard
                        icon={Sparkles}
                        title="人格配置"
                        detail="调整 Alice 的称呼、头像、协作风格、回答语言和常驻规则。"
                        action="打开人格配置"
                        onClick={() => setPersonaOpen(true)}
                      />
                      <ActionCard
                        icon={Wrench}
                        title="Skills 管理"
                        detail="安装、启停、编辑和迁移 SKILL.md，让助手按任务获得工具能力。"
                        action="管理 Skills"
                        onClick={() => setSkillsOpen(true)}
                      />
                    </div>

                    <div className="rounded-md border border-border/60 bg-surface-inset/25 px-4 py-3">
                      <div className="flex items-center gap-2">
                        <ClipboardCheck className="h-4 w-4 text-muted-foreground" />
                        <h4 className="text-sm font-semibold text-foreground">
                          注入链路
                        </h4>
                      </div>
                      <div className="mt-3 space-y-2.5">
                        {[
                          ["1", "人格规则", "形成 system prompt 的协作基线"],
                          [
                            "2",
                            "场景匹配",
                            "按写作、研究、检索等任务筛选 Skills",
                          ],
                          ["3", "工具确认", "写入、删除、迁移类操作进入确认流"],
                        ].map(([step, title, detail]) => (
                          <div key={step} className="flex gap-2.5">
                            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-background text-[10px] font-semibold text-muted-foreground">
                              {step}
                            </span>
                            <span className="min-w-0">
                              <span className="block text-xs font-medium text-foreground">
                                {title}
                              </span>
                              <span className="mt-0.5 block text-[11px] leading-relaxed text-muted-foreground">
                                {detail}
                              </span>
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>

                  <div className="grid gap-3 md:grid-cols-3">
                    {[
                      ["全局", "适合常用写作风格和通用工具。"],
                      ["当前库", "适合项目专属流程、术语和脚本。"],
                      ["确认", "高风险工具执行前会展示参数和影响。"],
                    ].map(([title, detail]) => (
                      <div
                        key={title}
                        className="rounded-md border border-border/50 bg-background/60 px-3 py-2.5"
                      >
                        <p className="text-xs font-semibold text-foreground">
                          {title}
                        </p>
                        <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
                          {detail}
                        </p>
                      </div>
                    ))}
                  </div>
                </section>
              ) : null}

              {activeSection === "memory" ? (
                <section>
                  <AiRulesPanel compact />
                </section>
              ) : null}
            </div>
          </ScrollArea>
        </div>
      </IrisOverlay>
      <PersonaSettingsPanel
        open={personaOpen}
        onClose={() => setPersonaOpen(false)}
      />
      <SkillsPanel open={skillsOpen} onClose={() => setSkillsOpen(false)} />
    </>
  );
}
