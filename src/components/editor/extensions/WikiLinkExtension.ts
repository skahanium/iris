import { Mark, mergeAttributes, type RawCommands } from "@tiptap/core";
import { ReactRenderer } from "@tiptap/react";
import Suggestion, {
  type SuggestionMatch,
  type SuggestionProps,
  type Trigger,
} from "@tiptap/suggestion";
import { Plugin, PluginKey } from "@tiptap/pm/state";

import { fileList } from "@/lib/ipc";
import {
  buildWikiLinkSuggestionItems,
  filterWikiLinkSuggestionItems,
  findWikiLinkSuggestionMatch,
  type WikiLinkSuggestionItem,
} from "@/lib/wiki-link-suggestions";

import {
  WikiLinkSuggestionList,
  type WikiLinkSuggestionListRef,
} from "../WikiLinkSuggestionList";

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

export interface WikiLinkOptions {
  HTMLAttributes: Record<string, unknown>;
  canMutate?: () => boolean;
  onOpenNote?: (title: string) => void;
  onPrepareNote?: (title: string) => void;
  getSuggestions?: () => Promise<WikiLinkSuggestionItem[]>;
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
      canMutate: () => true,
      onOpenNote: undefined,
      onPrepareNote: undefined,
      getSuggestions: async () =>
        buildWikiLinkSuggestionItems(await fileList()),
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
          if (!this.options.canMutate?.()) return false;
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
    const canMutate = this.options.canMutate ?? (() => true);
    const onPrepareNote = this.options.onPrepareNote;
    const getSuggestions =
      this.options.getSuggestions ??
      (async () => buildWikiLinkSuggestionItems(await fileList()));
    let cachedSuggestions: WikiLinkSuggestionItem[] | null = null;

    const loadSuggestions = async () => {
      if (cachedSuggestions) return cachedSuggestions;
      try {
        cachedSuggestions = await getSuggestions();
      } catch {
        cachedSuggestions = [];
      }
      return cachedSuggestions;
    };

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
          handleDOMEvents: {
            mouseover: (_view, event) => {
              const target = event.target as HTMLElement;
              const title = target
                .closest("[data-wiki-link]")
                ?.getAttribute("data-wiki-title");
              if (title) onPrepareNote?.(title);
              return false;
            },
            focusin: (_view, event) => {
              const target = event.target as HTMLElement;
              const title = target
                .closest("[data-wiki-link]")
                ?.getAttribute("data-wiki-title");
              if (title) onPrepareNote?.(title);
              return false;
            },
          },
        },
      }),
      new Plugin({
        key: new PluginKey("wikiLinkFullWidthTrigger"),
        appendTransaction: (transactions, _oldState, newState) => {
          if (!canMutate()) return null;
          if (!transactions.some((transaction) => transaction.docChanged)) {
            return null;
          }

          const { selection } = newState;
          if (!selection.empty) return null;

          const $from = selection.$from;
          if ($from.parent.type.name === "codeBlock") return null;

          const textBeforeCursor = $from.parent.textBetween(
            0,
            $from.parentOffset,
            undefined,
            "\ufffc",
          );
          if (!textBeforeCursor.endsWith("【【")) return null;

          return newState.tr.insertText(
            "[[",
            selection.from - 2,
            selection.from,
          );
        },
      }),
      Suggestion<WikiLinkSuggestionItem, WikiLinkSuggestionItem>({
        editor: this.editor,
        pluginKey: new PluginKey("wikiLinkSuggestion"),
        char: "[[",
        allowSpaces: true,
        allow: ({ editor, state, range }) => {
          if (!editor.isEditable || !canMutate()) return false;
          const $from = state.doc.resolve(range.from);
          return $from.parent.type.name !== "codeBlock";
        },
        findSuggestionMatch: ({ $position }: Trigger): SuggestionMatch => {
          const textBeforeCursor = $position.parent.textBetween(
            0,
            $position.parentOffset,
            undefined,
            "\ufffc",
          );
          const match = findWikiLinkSuggestionMatch(textBeforeCursor);
          if (!match) return null;

          return {
            range: {
              from: $position.pos - match.text.length,
              to: $position.pos,
            },
            query: match.query,
            text: match.text,
          };
        },
        items: async ({ query }) =>
          filterWikiLinkSuggestionItems(await loadSuggestions(), query),
        command: ({ editor, range, props }) => {
          if (!canMutate()) return;
          editor
            .chain()
            .focus()
            .deleteRange(range)
            .insertContentAt(range.from, {
              type: "text",
              marks: [{ type: this.name, attrs: { title: props.title } }],
              text: props.title,
            })
            .run();
        },
        render: () => {
          let component: ReactRenderer<WikiLinkSuggestionListRef> | null = null;
          let popup: SuggestionPopup[] | null = null;

          return {
            onStart: (props: SuggestionProps<WikiLinkSuggestionItem>) => {
              component = new ReactRenderer(WikiLinkSuggestionList, {
                props: {
                  items: props.items,
                  command: props.command,
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
            onUpdate(props: SuggestionProps<WikiLinkSuggestionItem>) {
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
              cachedSuggestions = null;
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
