import type { ComponentProps } from "react";

import { RuleConfirmDialog } from "./RuleConfirmDialog";
import { ToolConfirmDialog } from "./ToolConfirmDialog";

interface AssistantConfirmDialogsProps {
  ruleConfirmRequest: ComponentProps<typeof RuleConfirmDialog>["request"];
  toolConfirmRequest: ComponentProps<typeof ToolConfirmDialog>["request"];
  onRuleConfirm: ComponentProps<typeof RuleConfirmDialog>["onConfirm"];
  onRuleReject: ComponentProps<typeof RuleConfirmDialog>["onReject"];
  onRuleClose: ComponentProps<typeof RuleConfirmDialog>["onClose"];
  onToolConfirm: ComponentProps<typeof ToolConfirmDialog>["onConfirm"];
  onToolClose: ComponentProps<typeof ToolConfirmDialog>["onClose"];
}

export function AssistantConfirmDialogs({
  ruleConfirmRequest,
  toolConfirmRequest,
  onRuleConfirm,
  onRuleReject,
  onRuleClose,
  onToolConfirm,
  onToolClose,
}: AssistantConfirmDialogsProps) {
  return (
    <>
      <ToolConfirmDialog
        request={toolConfirmRequest}
        onConfirm={onToolConfirm}
        onClose={onToolClose}
      />
      <RuleConfirmDialog
        request={ruleConfirmRequest}
        onConfirm={onRuleConfirm}
        onReject={onRuleReject}
        onClose={onRuleClose}
      />
    </>
  );
}
