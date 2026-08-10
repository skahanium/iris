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
import { contextReferenceDisplayText } from "@/lib/context-reference";
import type { DisplayMention } from "@/types/ai";
import type { ContextReference } from "@/types/ai";
import type { McpCapabilityBindingSummary } from "@/lib/ipc";
import type { EditorSelectionCandidate } from "@/types/editor-selection";

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
  contextReferences: ContextReference[];
  editorSelectionCandidate?: EditorSelectionCandidate | null;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onCompositionStart: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onCompositionEnd: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onImagesChange: Dispatch<SetStateAction<ImageAttachment[]>>;
  onExternalBindingToggle: (bindingId: string) => void;
  onRemoveContextReference: (id: string) => void;
  onDismissEditorSelectionReference?: () => void;
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
  contextReferences,
  editorSelectionCandidate = null,
  textareaRef,
  onComposerKeyDown,
  onCompositionStart,
  onCompositionEnd,
  onImagesChange,
  onExternalBindingToggle,
  onRemoveContextReference,
  onDismissEditorSelectionReference,
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
      {editorSelectionCandidate ? (
        <div
          className="border-t border-border-subtle px-3 py-2"
          data-testid="editor-selection-candidate"
        >
          <div className="flex min-w-0 items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-2 py-1.5 text-xs text-foreground">
            <span
              className="min-w-0 flex-1 truncate"
              title={editorSelectionCandidate.preview}
            >
              {editorSelectionCandidate.preview || "当前选区"}
            </span>
            {editorSelectionCandidate.status !== "ready" ? (
              <span className="shrink-0 text-[11px] text-muted-foreground">
                {editorSelectionCandidate.status === "save_required"
                  ? "保存后可引用"
                  : editorSelectionCandidate.status === "invalid"
                    ? "无法引用"
                    : "校验中"}
              </span>
            ) : null}
            <button
              type="button"
              aria-label="移除当前选区引用"
              className="shrink-0 text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
              disabled={composerDisabled || streaming}
              onClick={onDismissEditorSelectionReference}
            >
              ×
            </button>
          </div>
        </div>
      ) : null}
      {contextReferences.length > 0 ? (
        <div
          className="border-t border-border-subtle px-3 py-2"
          data-testid="context-reference-boundary"
        >
          <div className="mb-1 flex items-center justify-between gap-2">
            <span className="text-xs font-medium text-foreground">
              已收集引用
            </span>
            <span className="text-[11px] text-muted-foreground">
              随本条问题一并发送
            </span>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {contextReferences.map((reference) => (
              <span
                key={reference.id}
                className="inline-flex max-w-[280px] items-center gap-1 rounded-md border border-border-subtle bg-background px-2 py-1 text-xs text-foreground"
                title={contextReferenceDisplayText(reference)}
              >
                <span className="truncate">
                  {contextReferenceDisplayText(reference)}
                </span>
                <button
                  type="button"
                  aria-label={`移除引用 ${reference.id}`}
                  className="ml-0.5 shrink-0 text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={composerDisabled || streaming}
                  onClick={() => onRemoveContextReference(reference.id)}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        </div>
      ) : null}
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
