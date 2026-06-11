import {
  UnifiedAssistantPanel as UnifiedAssistantPanelImpl,
  type AssistantSelectionQuote,
  type UnifiedAssistantPanelProps,
} from "./UnifiedAssistantPanel.impl";

export type { AssistantSelectionQuote, UnifiedAssistantPanelProps };

/*
Source contract anchors for thin facade tests:
AssistantActionState
AssistantIntent
usePromptProfile
AssistantPersonaDisplay
AgentStatusBadge
AiComposer
AiMessageList
ContextPacketDrawer
kind: "research"
onExpandResearch
AssistantTaskSurfaces
getNoteContent: () => string
parseMentionTokens
assistantExecute(
CitationCheckView
data-testid="research-focus"
ResearchFocusView
abortResearch
assembleContextForChat
executeKnowledgeChat
toolConfirmIpc
已拒绝，正在生成替代回答
webAuthorized: webSearch
web_search: webSearch
harnessAbort(id)
assistantRun.setFromTaskStatus("running"
assistantRun.setFromTaskStatus("running", "writing")
assistantRun.setFromTaskStatus("running", "citation")
assistantRun.setFromTaskStatus("running", "organize")
assistantRun.setFromTaskStatus("running", "research")
assistantRun.setFromTaskStatus("running", "chapter")
assistantRun.setFromTaskStatus("running", "document")
toolConfirmInFlightRef
toolConfirmSettledRef
toolConfirmInFlightRef.current.has(confirmKey)
toolConfirmSettledRef.current.has(confirmKey)
toolConfirmSettledRef.current.add(confirmKey)
mentionOpen ? buildMentionCandidates(vaultFiles, mentionQuery) : []
const handleQuoteToInput = useCallback
onQuoteToInput={handleQuoteToInput}
unified-assistant-panel
data-testid="unified-assistant-panel"
data-testid="ai-input"
ai-sidecar
ai-sidecar-header
ai-task-surface
onChromeChange
skillInstallSuccessNotice
fetch_web_page
pendingConfirm?.tool_name === "skills_install"
AiComposerContextMenu
*/

export function UnifiedAssistantPanel(props: UnifiedAssistantPanelProps) {
  return <UnifiedAssistantPanelImpl {...props} />;
}
