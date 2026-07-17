import type {
  Dispatch,
  CompositionEvent,
  KeyboardEvent,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";

import { AiComposer } from "@/components/ui/ai-composer";
import type { MentionCandidate } from "@/lib/ai-context-scope";
import type { DisplayMention } from "@/types/ai";

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
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onCompositionStart: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onCompositionEnd: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onImagesChange: Dispatch<SetStateAction<ImageAttachment[]>>;
  onMentionHighlight: (index: number) => void;
  onMentionSelect: (candidate: MentionCandidate) => void;
  onSubmit: () => void;
  onValueChange: (value: string) => void;
  onSelect: () => void;
  onStop: () => void;
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
  textareaRef,
  onComposerKeyDown,
  onCompositionStart,
  onCompositionEnd,
  onImagesChange,
  onMentionHighlight,
  onMentionSelect,
  onSubmit,
  onValueChange,
  onSelect,
  onStop,
}: AssistantComposerDockProps) {
  return (
    <div data-testid="ai-input">
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
