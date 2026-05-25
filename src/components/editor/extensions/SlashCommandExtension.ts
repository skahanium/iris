import { Extension } from "@tiptap/core";
import Suggestion from "@tiptap/suggestion";

export interface SlashCommandOptions {
  onCommand?: (command: string) => void;
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
        command: ({ editor, range, props }) => {
          const item = props as { id: string; label: string };
          editor.chain().focus().deleteRange(range).run();
          onCommand?.(item.id);
        },
        items: ({ query }) => {
          const all = [
            { id: "summarize", label: "总结" },
            { id: "outline", label: "生成大纲" },
            { id: "brainstorm", label: "头脑风暴" },
            { id: "fix-grammar", label: "修复语法" },
            { id: "translate", label: "翻译" },
          ];
          return all.filter((i) =>
            i.label.toLowerCase().includes(query.toLowerCase()),
          );
        },
        render: () => {
          let el: HTMLDivElement | null = null;
          return {
            onStart: (props) => {
              el = document.createElement("div");
              el.className =
                "z-50 rounded-md border border-border bg-panel p-1 shadow-lg text-sm";
              document.body.appendChild(el);
              props.items.forEach((item) => {
                const btn = document.createElement("button");
                btn.className =
                  "block w-full rounded px-3 py-1.5 text-left hover:bg-muted";
                btn.textContent = (item as { label: string }).label;
                btn.onclick = () => props.command(item);
                el?.appendChild(btn);
              });
            },
            onUpdate(props) {
              if (!el) return;
              el.innerHTML = "";
              props.items.forEach((item) => {
                const btn = document.createElement("button");
                btn.className =
                  "block w-full rounded px-3 py-1.5 text-left hover:bg-muted";
                btn.textContent = (item as { label: string }).label;
                btn.onclick = () => props.command(item);
                el?.appendChild(btn);
              });
            },
            onKeyDown(props) {
              if (props.event.key === "Escape") {
                el?.remove();
                return true;
              }
              return false;
            },
            onExit() {
              el?.remove();
            },
          };
        },
      }),
    ];
  },
});
