import type {
  Dispatch,
  KeyboardEvent,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";

import { AiComposer } from "@/components/ui/ai-composer";
import type { MentionCandidate } from "@/lib/ai-context-scope";

import type { ImageAttachment } from "./AiMessageList";
import { AiComposerContextMenu } from "./AiComposerContextMenu";
import { AiMentionPopover } from "./AiMentionPopover";

interface AssistantComposerDockProps {
  composerDisabled: boolean;
  images: ImageAttachment[];
  input: string;
  mentionCandidates: MentionCandidate[];
  mentionHighlight: number;
  mentionNavDeltaRef: MutableRefObject<1 | -1 | 0>;
  mentionOpen: boolean;
  mentionQuery: string;
  streaming: boolean;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onImagesChange: Dispatch<SetStateAction<ImageAttachment[]>>;
  onMentionHighlight: (index: number) => void;
  onMentionSelect: (candidate: MentionCandidate) => void;
  onSubmit: () => void;
  onValueChange: Dispatch<SetStateAction<string>>;
  onSelect: () => void;
  onStop: () => void;
}

export function AssistantComposerDock({
  composerDisabled,
  images,
  input,
  mentionCandidates,
  mentionHighlight,
  mentionNavDeltaRef,
  mentionOpen,
  mentionQuery,
  streaming,
  textareaRef,
  onComposerKeyDown,
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
          streaming={streaming}
          disabled={composerDisabled}
          placeholder="输入问题，或直接说明你想查、想改、想检、想整理什么"
          textareaRef={textareaRef}
          onComposerKeyDown={onComposerKeyDown}
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
