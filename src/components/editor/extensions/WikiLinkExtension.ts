import { Mark, mergeAttributes, type RawCommands } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export interface WikiLinkOptions {
  HTMLAttributes: Record<string, unknown>;
  onOpenNote?: (title: string) => void;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    wikiLink: {
      insertWikiLink: (title: string) => ReturnType;
    };
  }
}

/**
 * Custom mark for [[wiki-link]] syntax.
 * Renders as a link with accent color. On click, navigates to the target note.
 */
export const WikiLinkExtension = Mark.create<WikiLinkOptions>({
  name: "wikiLink",

  priority: 1000,

  addOptions() {
    return {
      HTMLAttributes: {},
      onOpenNote: undefined,
    };
  },

  addAttributes() {
    return {
      title: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-wiki-title"),
        renderHTML: (attrs) => ({
          "data-wiki-title": attrs.title as string,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-wiki-link]" }];
  },

  renderHTML({ HTMLAttributes }) {
    const title = HTMLAttributes["data-wiki-title"] as string | undefined;
    return [
      "span",
      mergeAttributes(
        { "data-wiki-link": "", class: "wiki-link" },
        HTMLAttributes,
      ),
      ["span", { class: "wiki-link-bracket" }, "[["],
      title ?? "",
      ["span", { class: "wiki-link-bracket" }, "]]"],
    ];
  },

  addCommands(): Partial<RawCommands> {
    return {
      insertWikiLink:
        (title: string) =>
        ({ chain, state }) => {
          const { from, to } = state.selection;
          return chain()
            .deleteRange({ from, to })
            .insertContentAt(from, {
              type: "text",
              marks: [{ type: this.name, attrs: { title } }],
              text: title,
            })
            .run();
        },
    } as Partial<RawCommands>;
  },

  addProseMirrorPlugins() {
    const onOpenNote = this.options.onOpenNote;

    return [
      new Plugin({
        key: new PluginKey("wikiLinkClick"),
        props: {
          handleClick: (_view, _pos, event) => {
            const target = event.target as HTMLElement;
            const wikiEl = target.closest("[data-wiki-link]");
            if (!wikiEl) return false;

            const title = wikiEl.getAttribute("data-wiki-title");
            if (title && onOpenNote) {
              event.preventDefault();
              onOpenNote(title);
              return true;
            }
            return false;
          },
        },
      }),
    ];
  },
});
