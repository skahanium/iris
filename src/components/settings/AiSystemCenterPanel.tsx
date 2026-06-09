import { useState } from "react";

import { AiRulesPanel } from "@/components/ai/AiRulesPanel";
import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { LlmRoutingSection } from "@/components/settings/LlmRoutingSection";
import { MinimaxSearchSection } from "@/components/settings/MinimaxSearchSection";
import { PersonaSettingsPanel } from "@/components/settings/PersonaSettingsPanel";
import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";

interface AiSystemCenterPanelProps {
  open: boolean;
  onClose: () => void;
}

export function AiSystemCenterPanel({
  open,
  onClose,
}: AiSystemCenterPanelProps) {
  const [personaOpen, setPersonaOpen] = useState(false);
  const [skillsOpen, setSkillsOpen] = useState(false);

  return (
    <>
      <IrisOverlay
        open={open}
        onClose={onClose}
        title="AI 系统中心"
        size="wide"
      >
        <div data-testid="ai-system-center" className="flex min-h-0 flex-1">
          <aside className="w-48 shrink-0 border-r border-border/60 bg-surface-inset/20 p-3 text-xs text-muted-foreground">
            模型 · 联网 · 人格 · Skills · 记忆
          </aside>
          <ScrollArea className="min-h-0 flex-1">
            <div className="space-y-6 px-5 py-4">
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  模型路由
                </h3>
                <LlmRoutingSection open={open} />
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  联网搜索
                </h3>
                <MinimaxSearchSection open={open} />
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  人格与 Skills
                </h3>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => setPersonaOpen(true)}
                  >
                    打开人格配置
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => setSkillsOpen(true)}
                  >
                    管理 Skills
                  </Button>
                </div>
              </section>
              <section>
                <h3 className="mb-2 text-sm font-medium text-foreground">
                  AI 记忆与规则
                </h3>
                <AiRulesPanel compact />
              </section>
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
