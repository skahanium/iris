import { UnifiedAssistantPanel as UnifiedAssistantPanelImpl } from "./UnifiedAssistantPanel.impl";
import type { UnifiedAssistantPanelProps } from "./types";

export type { UnifiedAssistantPanelProps };

/** Public assistant panel facade. Execution is owned by the unified Run controller. */
export function UnifiedAssistantPanel(props: UnifiedAssistantPanelProps) {
  return <UnifiedAssistantPanelImpl {...props} />;
}
