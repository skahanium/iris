import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion, { type SuggestionProps } from "@tiptap/suggestion";

import { buildSlashItemsFromContext } from "@/lib/slash-commands";
import { editorHasActiveAiStream } from "@/lib/editor-ai-stream";

import {
  SlashCommandList,
  type SlashCommandListRef,
  type SlashItem,
} from "../SlashCommandList";

interface SuggestionPopup {
  destroy: () => void;
  hide: () => void;
  setProps: (props: { getReferenceClientRect: () => DOMRect }) => void;
}

async function loadTippy() {
  void import("tippy.js/dist/tippy.css").catch(() => undefined);
  const { default: tippy } = await import("tippy.js");
  return tippy;
}

export interface SlashCommandOptions {
  canMutate?: () => boolean;
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
    return { canMutate: () => true, onCommand: undefined };
  },

  addProseMirrorPlugins() {
    const onCommand = this.options.onCommand;
    const canMutate = this.options.canMutate ?? (() => true);

    return [
      Suggestion({
        editor: this.editor,
        char: "/",
        allow: () => true,
        command: ({ editor, range, props }) => {
          if (!canMutate()) return;
          const item = props as SlashItem;
          const pos = range.from;
          editor.chain().focus().deleteRange(range).setTextSelection(pos).run();
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
          let popup: SuggestionPopup[] | null = null;

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

              void loadTippy().then((tippy) => {
                if (!component || !props.clientRect) return;
                popup = tippy("body", {
                  getReferenceClientRect: props.clientRect as () => DOMRect,
                  appendTo: () => document.body,
                  content: component.element,
                  showOnCreate: true,
                  interactive: true,
                  trigger: "manual",
                  theme: "iris-suggestion",
                  arrow: false,
                  maxWidth: "none",
                  offset: [0, 6],
                  placement: "bottom-start",
                });
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
