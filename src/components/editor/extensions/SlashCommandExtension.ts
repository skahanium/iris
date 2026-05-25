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
            { id: "summarize", label: "总结", icon: "FileText" },
            { id: "outline", label: "生成大纲", icon: "ListTree" },
            { id: "brainstorm", label: "头脑风暴", icon: "Lightbulb" },
            { id: "fix-grammar", label: "修复语法", icon: "Languages" },
            { id: "translate", label: "翻译", icon: "Globe" },
          ];
          return all.filter((i) =>
            i.label.toLowerCase().includes(query.toLowerCase()),
          );
        },
        render: () => {
          let el: HTMLDivElement | null = null;
          const iconMap: Record<string, string> = {
            FileText: "📄",
            ListTree: "🌲",
            Lightbulb: "💡",
            Languages: "🔤",
            Globe: "🌐",
          };
          return {
            onStart: (props) => {
              el = document.createElement("div");
              el.className =
                "z-50 rounded-md border border-primary/20 bg-chrome shadow-lg text-sm";
              document.body.appendChild(el);
              props.items.forEach((item) => {
                const it = item as { id: string; label: string; icon?: string };
                const btn = document.createElement("button");
                btn.className =
                  "flex w-full items-center gap-2 rounded px-3 py-1.5 text-left hover:bg-muted";
                const icon = document.createElement("span");
                icon.className = "text-xs";
                icon.textContent = iconMap[it.icon ?? ""] ?? "";
                btn.appendChild(icon);
                const label = document.createElement("span");
                label.textContent = it.label;
                btn.appendChild(label);
                btn.onclick = () => props.command(item);
                el?.appendChild(btn);
              });
            },
            onUpdate(props) {
              if (!el) return;
              el.innerHTML = "";
              props.items.forEach((item) => {
                const it = item as { id: string; label: string; icon?: string };
                const btn = document.createElement("button");
                btn.className =
                  "flex w-full items-center gap-2 rounded px-3 py-1.5 text-left hover:bg-muted";
                const icon = document.createElement("span");
                icon.className = "text-xs";
                icon.textContent = iconMap[it.icon ?? ""] ?? "";
                btn.appendChild(icon);
                const label = document.createElement("span");
                label.textContent = it.label;
                btn.appendChild(label);
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
