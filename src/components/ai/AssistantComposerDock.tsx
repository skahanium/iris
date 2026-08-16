import {
  useRef,
  type Dispatch,
  type SetStateAction,
  type RefObject,
} from "react";

import type { AssistantComposerHandle } from "@/components/ui/ai-composer";
import { cn } from "@/lib/utils";
import type { MentionCandidate, MentionTextEdit } from "@/lib/ai-context-scope";
import type { McpCapabilityBindingSummary } from "@/lib/ipc";
import type {
  ContextReference,
  DisplayMention,
  SecurityDomain,
} from "@/types/ai";
import type { EditorSelectionCandidate } from "@/types/editor-selection";

import type { ImageAttachment } from "./AiMessageList";
import { AiComposerContextMenu } from "./AiComposerContextMenu";
import { AssistantAiComposer } from "./AssistantAiComposer";
import { AssistantContextShelf } from "./AssistantContextShelf";

interface AssistantComposerDockProps {
  composerDisabled: boolean;
  images: ImageAttachment[];
  input: string;
  displayMentions: DisplayMention[];
  streaming: boolean;
  externalBindings: McpCapabilityBindingSummary[];
  selectedExternalBindingIds: string[];
  contextReferences: ContextReference[];
  editorSelectionCandidate?: EditorSelectionCandidate | null;
  composerRef?: RefObject<AssistantComposerHandle | null>;
  domain?: SecurityDomain;
  getMentionCandidates?: (
    prefix: "@" | "#",
    query: string,
  ) => MentionCandidate[];
  onImagesChange: Dispatch<SetStateAction<ImageAttachment[]>>;
  onExternalBindingToggle: (bindingId: string) => void;
  onRemoveContextReference: (id: string) => void;
  onDismissEditorSelectionReference?: (candidateKey: string) => void;
  onSubmit: () => void;
  onValueChange: (
    value: string,
    mentions?: DisplayMention[] | MentionTextEdit,
  ) => void;
  onStop: () => void;
  assistantFocus?: boolean;
  onComposerKeyDown?: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  onCompositionStart?: (
    event: React.CompositionEvent<HTMLTextAreaElement>,
  ) => void;
  onCompositionEnd?: (
    event: React.CompositionEvent<HTMLTextAreaElement>,
  ) => void;
  onSelect?: () => void;
  onMentionHighlight?: (index: number) => void;
  onMentionSelect?: (candidate: MentionCandidate) => void;
  setMentionHighlight?: (index: number) => void;
  /** Compatibility props for callers migrating from the textarea Composer. */
  mentionCandidates?: MentionCandidate[];
  mentionHighlight?: number;
  mentionNavDeltaRef?: React.MutableRefObject<1 | -1 | 0>;
  mentionOpen?: boolean;
  mentionPrefix?: "@" | "#";
  mentionQuery?: string;
  textareaRef?: RefObject<HTMLTextAreaElement | null>;
}

export function AssistantComposerDock({
  composerDisabled,
  images,
  input,
  streaming,
  externalBindings,
  selectedExternalBindingIds,
  contextReferences,
  editorSelectionCandidate = null,
  composerRef,
  domain,
  getMentionCandidates,
  onImagesChange,
  onExternalBindingToggle,
  onRemoveContextReference,
  onDismissEditorSelectionReference,
  onSubmit,
  onValueChange,
  onStop,
  assistantFocus = false,
}: AssistantComposerDockProps) {
  const internalComposerRef = useRef<AssistantComposerHandle | null>(null);
  const activeComposerRef = composerRef ?? internalComposerRef;
  const activeDomain = domain ?? "normal";
  const contextShelf = (
    <AssistantContextShelf
      candidate={editorSelectionCandidate}
      contextReferences={contextReferences}
      composerDisabled={composerDisabled}
      streaming={streaming}
      onDismissCandidate={onDismissEditorSelectionReference}
      onRemoveReference={onRemoveContextReference}
    />
  );

  return (
    <div
      data-testid="ai-input"
      className={cn(
        "flex shrink-0 flex-col",
        assistantFocus && "ai-focus-column",
      )}
    >
      {externalBindings.length > 0 ? (
        <div
          className="border-t border-border-subtle px-3 py-2"
          data-testid="external-tool-grant-boundary"
        >
          <div className="mb-1 flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-foreground">
              本次外部只读工具
            </span>
            <span className="text-[11px] text-muted-foreground">
              发送后自动清除授权
            </span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {externalBindings.map((binding) => {
              const selected = selectedExternalBindingIds.includes(binding.id);
              return (
                <button
                  key={binding.id}
                  type="button"
                  className={cn(
                    "rounded-md border px-2 py-1 text-xs transition-colors",
                    selected
                      ? "border-primary/40 bg-primary/10 text-foreground"
                      : "border-border-subtle bg-transparent text-muted-foreground hover:bg-surface-inset",
                  )}
                  disabled={composerDisabled || streaming}
                  aria-pressed={selected}
                  onClick={() => onExternalBindingToggle(binding.id)}
                >
                  {binding.mcpToolName}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
      <AiComposerContextMenu composerRef={activeComposerRef}>
        <AssistantAiComposer
          value={input}
          composerRef={activeComposerRef}
          domain={activeDomain}
          getMentionCandidates={getMentionCandidates}
          onChange={(value, mentions) => onValueChange(value, mentions)}
          onSubmit={onSubmit}
          onStop={onStop}
          streaming={streaming}
          disabled={composerDisabled}
          submitDisabled={
            editorSelectionCandidate !== null &&
            editorSelectionCandidate.status !== "ready"
          }
          contextShelf={contextShelf}
          images={images}
          onImagesChange={onImagesChange}
        />
      </AiComposerContextMenu>
    </div>
  );
}
