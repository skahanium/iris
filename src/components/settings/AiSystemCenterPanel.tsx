import { useState } from "react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { Button } from "@/components/ui/button";
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
                <section className="space-y-3">
                  <div className="grid gap-2 sm:grid-cols-2">
                    <Button
                      type="button"
                      variant="outline"
                      className="justify-start"
                      onClick={() => setPersonaOpen(true)}
                    >
                      打开人格配置
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      className="justify-start"
                      onClick={() => setSkillsOpen(true)}
                    >
                      管理 Skills
                    </Button>
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
