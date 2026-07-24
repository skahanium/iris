import { Check, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { AssistantRunConfirmation as AssistantRunConfirmationState } from "@/hooks/useAssistantRun";

export interface AssistantRunConfirmationProps {
  confirmation: AssistantRunConfirmationState;
  disabled?: boolean;
  onApprove: () => void;
  onReject: () => void;
}

function effectLabel(effect: AssistantRunConfirmationState["effect"]): string {
  switch (effect) {
    case "answer":
      return "回答";
    case "draft":
      return "起草";
    case "apply":
      return "应用更改";
    default:
      return "执行此操作";
  }
}

/** Renders the persisted, safe change-plan projection before a Run can resume. */
export function AssistantRunConfirmation({
  confirmation,
  disabled = false,
  onApprove,
  onReject,
}: AssistantRunConfirmationProps) {
  return (
    <section
      className="border-b border-warning/30 bg-warning-bg px-3 py-2"
      data-testid="assistant-run-confirmation"
      aria-live="polite"
    >
      <p className="text-xs font-medium">需要确认</p>
      <p className="mt-1 text-xs text-muted-foreground">
        {confirmation.summary}
      </p>
      {confirmation.targets?.length ? (
        <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
          {confirmation.targets.map((target, index) => (
            <li key={`${target.kind}:${target.label}:${index}`}>
              {target.label} · {target.risk}
              {target.detail ? ` · ${target.detail}` : ""}
            </li>
          ))}
        </ul>
      ) : null}
      {confirmation.expiresAt ? (
        <p className="mt-2 text-[11px] text-muted-foreground">
          确认有效期至：{confirmation.expiresAt}
        </p>
      ) : null}
      <div className="mt-2 flex gap-2">
        <Button
          type="button"
          size="sm"
          className="h-7 gap-1 text-xs"
          disabled={disabled}
          onClick={onApprove}
        >
          <Check className="h-3.5 w-3.5" />
          {effectLabel(confirmation.effect)}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-7 gap-1 text-xs"
          disabled={disabled}
          onClick={onReject}
        >
          <X className="h-3.5 w-3.5" />
          拒绝
        </Button>
      </div>
    </section>
  );
}
