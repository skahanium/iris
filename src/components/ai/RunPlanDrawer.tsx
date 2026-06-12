import { X } from "lucide-react";

import { Button } from "@/components/ui/button";
import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

interface RunPlanDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  runPlanSummary: AgentRunPlanSummary | null;
  intentDetection: IntentDetectionResult | null;
  permissionPreflightSummary: PermissionPreflightSummary | null;
}

function listOrEmpty(items: string[] | undefined, empty: string): string[] {
  return items && items.length > 0 ? items : [empty];
}

export function RunPlanDrawer({
  open,
  onOpenChange,
  runPlanSummary,
  intentDetection,
  permissionPreflightSummary,
}: RunPlanDrawerProps) {
  if (!open) return null;

  const intent =
    intentDetection?.detectedIntent ?? runPlanSummary?.detectedIntent ?? "chat";
  const confidence =
    intentDetection?.confidence === undefined
      ? "unknown"
      : `${Math.round(intentDetection.confidence * 100)}%`;
  const contextItems = listOrEmpty(
    runPlanSummary?.contextSummary,
    "No extra context",
  );
  const blockedItems = listOrEmpty(runPlanSummary?.blockedReasons, "No blocks");
  const alternatives = listOrEmpty(intentDetection?.alternatives, "No actions");
  const modelRoute = runPlanSummary?.modelRoute;
  const personaLayers = listOrEmpty(
    runPlanSummary?.personaLayers?.map(
      (layer) => `${layer.layer}: ${layer.summary}`,
    ),
    "Waiting for persona summary",
  );
  const skillActivationPlan = runPlanSummary?.skillActivationPlan;
  const skillItems =
    skillActivationPlan?.activatedSkills.map(
      (skill) =>
        `${skill.name} (${skill.matchReason}, score ${skill.score.toFixed(2)})`,
    ) ?? [];
  const blockedCapabilities = runPlanSummary?.blockedCapabilities ?? [];
  const requiredConfirmations =
    permissionPreflightSummary?.requiredConfirmations ?? [];
  const exposedTools = permissionPreflightSummary?.exposedTools ?? [];

  return (
    <div
      data-testid="run-plan-drawer"
      className="fixed inset-y-0 right-0 z-50 flex w-full max-w-md flex-col border-l border-border bg-background shadow-overlay"
      role="dialog"
      aria-label="Run Plan"
    >
      <div className="flex items-center justify-between border-b border-border/60 px-4 py-3">
        <div>
          <p className="text-xs font-medium text-foreground">Run Plan</p>
          <p className="text-[11px] text-muted-foreground">
            Intent, model, persona, skills, permissions
          </p>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          aria-label="Close Run Plan"
          onClick={() => onOpenChange(false)}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4 text-xs">
        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Intent
          </h3>
          <p className="text-foreground">{intent}</p>
          <p className="text-muted-foreground">confidence: {confidence}</p>
          <p className="text-muted-foreground">
            {intentDetection?.reason ?? "Waiting for intent detection"}
          </p>
          <p className="text-muted-foreground">
            fallback:{" "}
            {intentDetection?.fallbackBehavior ?? "Waiting for fallback"}
          </p>
          <div className="space-y-1">
            <p className="text-muted-foreground">alternatives</p>
            <ul className="space-y-1">
              {alternatives.map((item) => (
                <li key={item} className="text-muted-foreground">
                  {item}
                </li>
              ))}
            </ul>
          </div>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Model
          </h3>
          <p className="text-foreground">
            {modelRoute
              ? `${modelRoute.slot} ${modelRoute.providerId}/${modelRoute.model}`
              : "Waiting for model route"}
          </p>
          <p className="text-muted-foreground">
            {modelRoute?.reason ?? "Waiting for route reason"}
          </p>
          <p className="text-muted-foreground">
            probe: {modelRoute?.probeStatus ?? "unknown"}
          </p>
          <p className="text-muted-foreground">
            fallback: {modelRoute?.fallbackChain?.join(" -> ") ?? "pending"}
          </p>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Persona
          </h3>
          <ul className="space-y-1">
            {personaLayers.map((item) => (
              <li key={item} className="text-muted-foreground">
                {item}
              </li>
            ))}
          </ul>
          <p className="text-muted-foreground">
            Persona affects expression, not permissions.
          </p>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Skills
          </h3>
          <p className="text-muted-foreground">
            {skillActivationPlan?.skillOverlaySummary ??
              "Waiting for skill activation"}
          </p>
          <ul className="space-y-1">
            {listOrEmpty(skillItems, "No skills activated").map((item) => (
              <li key={item} className="text-muted-foreground">
                {item}
              </li>
            ))}
          </ul>
          {blockedCapabilities.length > 0 ? (
            <div className="space-y-1">
              <p className="text-muted-foreground">blocked capabilities</p>
              <ul className="space-y-1">
                {blockedCapabilities.map((blocked) => (
                  <li
                    key={`${blocked.skillName}-${blocked.capability}`}
                    className="text-muted-foreground"
                  >
                    {blocked.skillName}: {blocked.capability} / {blocked.status}{" "}
                    / {blocked.fallbackGuidance}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Context
          </h3>
          <ul className="space-y-1">
            {contextItems.map((item) => (
              <li key={item} className="text-muted-foreground">
                {item}
              </li>
            ))}
          </ul>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Permissions
          </h3>
          <p className="text-muted-foreground">
            {permissionPreflightSummary?.summary ??
              runPlanSummary?.permissionSummary ??
              "Waiting for permission preflight"}
          </p>
          {requiredConfirmations.length > 0 ? (
            <p className="text-muted-foreground">
              confirmations: {requiredConfirmations.join(", ")}
            </p>
          ) : null}
          {exposedTools.length > 0 ? (
            <p className="text-muted-foreground">
              skill tools: {exposedTools.join(", ")}
            </p>
          ) : null}
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Progress
          </h3>
          <p className="text-muted-foreground">
            {runPlanSummary?.progressState ?? "idle"}
          </p>
          <p className="text-muted-foreground">
            {runPlanSummary?.degraded ? "Degraded" : "Normal"}
          </p>
        </section>

        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold uppercase tracking-normal text-muted-foreground">
            Blocks
          </h3>
          <ul className="space-y-1">
            {blockedItems.map((item) => (
              <li key={item} className="text-muted-foreground">
                {item}
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
