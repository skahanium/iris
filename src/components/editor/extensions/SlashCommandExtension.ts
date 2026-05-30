import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion, { type SuggestionProps } from "@tiptap/suggestion";
import tippy, { type Instance as TippyInstance } from "tippy.js";

import { buildSlashItemsFromContext } from "@/lib/slash-commands";
import { editorHasActiveAiStream } from "@/lib/editor-ai-stream";

import {
  SlashCommandList,
  type SlashCommandListRef,
  type SlashItem,
} from "../SlashCommandList";

export interface SlashCommandOptions {
  onCommand?: (command: string) => void;
  hasNote?: () => boolean;
}

function slashItemsForEditor(
  editor: import("@tiptap/core").Editor,
): SlashItem[] {
  const { from, to } = editor.state.selection;
  const ctx = {
    hasNote: true,
    hasSelection: from !== to,
    streaming: editorHasActiveAiStream(editor),
  };
  return buildSlashItemsFromContext(ctx);
}

export const SlashCommandExtension = Extension.create<SlashCommandOptions>({
  name: "slashCommand",

  addOptions() {
    return { onCommand: undefined };
  },

  addProseMirrorPlugins() {
    const onCommand = this.options.onCommand;

    return [
      Suggestion({
        editor: this.editor,
        char: "/",
        allow: () => true,
        command: ({ editor, range, props }) => {
          const item = props as SlashItem;
          editor.chain().focus().deleteRange(range).run();
          onCommand?.(item.id);
        },
        items: ({ query, editor: ed }) => {
          const items = slashItemsForEditor(ed);
          const q = query.toLowerCase();
          return items.filter((i) => {
            const hay = `${i.label} ${i.id} ${i.keywords ?? ""}`.toLowerCase();
            return hay.includes(q);
          });
        },
        render: () => {
          let component: ReactRenderer<SlashCommandListRef> | null = null;
          let popup: TippyInstance[] | null = null;

          return {
            onStart: (props: SuggestionProps<SlashItem>) => {
              const { from, to } = props.editor.state.selection;
              const selectionHint = from !== to;
              component = new ReactRenderer(SlashCommandList, {
                props: {
                  items: props.items,
                  command: props.command,
                  selectionHint,
                },
                editor: props.editor,
              });

              if (!props.clientRect) return;

              popup = tippy("body", {
                getReferenceClientRect: props.clientRect as () => DOMRect,
                appendTo: () => document.body,
                content: component.element,
                showOnCreate: true,
                interactive: true,
                trigger: "manual",
                placement: "bottom-start",
              });
            },
            onUpdate(props: SuggestionProps<SlashItem>) {
              const { from, to } = props.editor.state.selection;
              component?.updateProps({
                items: props.items,
                command: props.command,
                selectionHint: from !== to,
              });
              if (props.clientRect && popup?.[0]) {
                popup[0].setProps({
                  getReferenceClientRect: props.clientRect as () => DOMRect,
                });
              }
            },
            onKeyDown(props: { event: KeyboardEvent }) {
              if (props.event.key === "Escape") {
                popup?.[0]?.hide();
                return true;
              }
              return component?.ref?.onKeyDown(props) ?? false;
            },
            onExit() {
              popup?.[0]?.destroy();
              component?.destroy();
              popup = null;
              component = null;
            },
          };
        },
      }),
    ];
  },
});
