import { Extension } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion, { type SuggestionProps } from "@tiptap/suggestion";
import tippy, { type Instance as TippyInstance } from "tippy.js";

import {
  SlashCommandList,
  type SlashCommandListRef,
  type SlashItem,
} from "../SlashCommandList";

export interface SlashCommandOptions {
  onCommand?: (command: string) => void;
}

const ALL_ITEMS: SlashItem[] = [
  { id: "summarize", label: "总结", icon: "FileText" },
  { id: "outline", label: "生成大纲", icon: "ListTree" },
  { id: "brainstorm", label: "头脑风暴", icon: "Lightbulb" },
  { id: "fix-grammar", label: "修复语法", icon: "Languages" },
  { id: "translate", label: "翻译", icon: "Globe" },
];

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
        command: ({ editor, range, props }) => {
          const item = props as SlashItem;
          editor.chain().focus().deleteRange(range).run();
          onCommand?.(item.id);
        },
        items: ({ query }) =>
          ALL_ITEMS.filter((i) =>
            i.label.toLowerCase().includes(query.toLowerCase()),
          ),
        render: () => {
          let component: ReactRenderer<SlashCommandListRef> | null = null;
          let popup: TippyInstance[] | null = null;

          return {
            onStart: (props: SuggestionProps<SlashItem>) => {
              component = new ReactRenderer(SlashCommandList, {
                props: {
                  items: props.items,
                  command: props.command,
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
              component?.updateProps({
                items: props.items,
                command: props.command,
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
