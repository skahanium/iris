import type {
  Dispatch,
  CompositionEvent,
  KeyboardEvent,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";

import { AiComposer } from "@/components/ui/ai-composer";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { MentionCandidate, MentionTextEdit } from "@/lib/ai-context-scope";
import type { DisplayMention } from "@/types/ai";
import type { McpCapabilityBindingSummary } from "@/lib/ipc";

import type { ImageAttachment } from "./AiMessageList";
import { AiComposerContextMenu } from "./AiComposerContextMenu";
import { AiMentionPopover } from "./AiMentionPopover";

interface AssistantComposerDockProps {
  composerDisabled: boolean;
  images: ImageAttachment[];
  input: string;
  displayMentions: DisplayMention[];
  mentionCandidates: MentionCandidate[];
  mentionHighlight: number;
  mentionNavDeltaRef: MutableRefObject<1 | -1 | 0>;
  mentionOpen: boolean;
  mentionPrefix: "@" | "#";
  mentionQuery: string;
  streaming: boolean;
  externalBindings: McpCapabilityBindingSummary[];
  selectedExternalBindingIds: string[];
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onCompositionStart: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onCompositionEnd: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onImagesChange: Dispatch<SetStateAction<ImageAttachment[]>>;
  onExternalBindingToggle: (bindingId: string) => void;
  onMentionHighlight: (index: number) => void;
  onMentionSelect: (candidate: MentionCandidate) => void;
  onSubmit: () => void;
  onValueChange: (value: string, edit?: MentionTextEdit) => void;
  onSelect: () => void;
  onStop: () => void;
  /** Agent 主区阅读：Composer 与授权边界进入最大 --ai-focus-measure 内容列（§7.3）。 */
  assistantFocus?: boolean;
}

export function AssistantComposerDock({
  composerDisabled,
  images,
  input,
  displayMentions,
  mentionCandidates,
  mentionHighlight,
  mentionNavDeltaRef,
  mentionOpen,
  mentionPrefix,
  mentionQuery,
  streaming,
  externalBindings,
  selectedExternalBindingIds,
  textareaRef,
  onComposerKeyDown,
  onCompositionStart,
  onCompositionEnd,
  onImagesChange,
  onExternalBindingToggle,
  onMentionHighlight,
  onMentionSelect,
  onSubmit,
  onValueChange,
  onSelect,
  onStop,
  assistantFocus = false,
}: AssistantComposerDockProps) {
  return (
    <div
      data-testid="ai-input"
      className={cn("flex flex-col", assistantFocus && "ai-focus-column")}
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
                <Button
                  key={binding.id}
                  type="button"
                  size="sm"
                  variant={selected ? "secondary" : "outline"}
                  disabled={composerDisabled || streaming}
                  aria-pressed={selected}
                  onClick={() => onExternalBindingToggle(binding.id)}
                >
                  {binding.mcpToolName}
                </Button>
              );
            })}
          </div>
        </div>
      ) : null}
      <AiComposerContextMenu
        textareaRef={textareaRef}
        value={input}
        onValueChange={onValueChange}
      >
        <AiComposer
          value={input}
          displayMentions={displayMentions}
          streaming={streaming}
          disabled={composerDisabled}
          placeholder="输入问题，或直接说明你想查、想改、想检、想整理什么"
          textareaRef={textareaRef}
          onComposerKeyDown={onComposerKeyDown}
          onCompositionStart={onCompositionStart}
          onCompositionEnd={onCompositionEnd}
          onSelect={onSelect}
          onChange={onValueChange}
          onSubmit={onSubmit}
          onStop={onStop}
          images={images}
          onImagesChange={onImagesChange}
          mentionPopover={
            <AiMentionPopover
              open={mentionOpen}
              query={mentionQuery}
              prefix={mentionPrefix}
              candidates={mentionCandidates}
              highlight={mentionHighlight}
              onHighlight={onMentionHighlight}
              navDeltaRef={mentionNavDeltaRef}
              onSelect={onMentionSelect}
            />
          }
        />
      </AiComposerContextMenu>
    </div>
  );
}
