import StarterKit from "@tiptap/starter-kit";
import type { AnyExtension } from "@tiptap/core";

import { AssistantMentionExtension } from "./AssistantMentionExtension";

interface AssistantComposerExtensionOptions {
  mentionExtension?: AnyExtension;
}

/** Minimal, plain-text-first schema used by the Agent Composer. */
export function createAssistantComposerExtensions({
  mentionExtension = AssistantMentionExtension,
}: AssistantComposerExtensionOptions = {}): AnyExtension[] {
  return [
    StarterKit.configure({
      blockquote: false,
      bold: false,
      bulletList: false,
      code: false,
      codeBlock: false,
      dropcursor: false,
      gapcursor: false,
      heading: false,
      horizontalRule: false,
      italic: false,
      listItem: false,
      orderedList: false,
      strike: false,
    }),
    mentionExtension,
  ];
}
