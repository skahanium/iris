import { mergeAttributes, Node } from "@tiptap/core";
import { PluginKey } from "@tiptap/pm/state";
import { ReactNodeViewRenderer, ReactRenderer } from "@tiptap/react";
import Suggestion from "@tiptap/suggestion";

import type { MentionCandidate } from "@/lib/ai-context-scope";
import { AssistantMentionNodeView } from "../AssistantMentionNodeView";
import {
  AiMentionPopover,
  type AiMentionPopoverRef,
} from "../AiMentionPopover";

export interface AssistantMentionExtensionOptions {
  getCandidates: (prefix: "@" | "#", query: string) => MentionCandidate[];
  enabled?: () => boolean;
}

interface SuggestionPopup {
  destroy: () => void;
  setProps: (props: { getReferenceClientRect: () => DOMRect }) => void;
}

export const assistantMentionPluginKeys = {
  at: new PluginKey("assistantMention-at"),
  hash: new PluginKey("assistantMention-hash"),
};

async function loadTippy() {
  void import("tippy.js/dist/tippy.css").catch(() => undefined);
  const { default: tippy } = await import("tippy.js");
  return tippy;
}

function isMentionBoundary(
  editor: import("@tiptap/core").Editor,
  from: number,
) {
  const preceding = editor.state.doc.textBetween(
    Math.max(0, from - 1),
    from,
    "\n",
    "\n",
  );
  return !preceding || /[\s([{「（【〔［《〈“‘]/u.test(preceding);
}

function normalizeCandidate(candidate: MentionCandidate): MentionCandidate {
  return {
    ...candidate,
    value:
      candidate.kind === "folder"
        ? candidate.value.replace(/\\/g, "/").replace(/\/?$/u, "/")
        : candidate.value.replace(/\\/g, "/"),
  };
}

export function insertAssistantMention(
  editor: import("@tiptap/core").Editor,
  candidate: MentionCandidate,
): boolean {
  const key =
    candidate.kind === "tag"
      ? assistantMentionPluginKeys.hash
      : assistantMentionPluginKeys.at;
  const state = key.getState(editor.state) as
    | { active: boolean; range: { from: number; to: number } }
    | undefined;
  if (!state?.active) return false;
  const normalized = normalizeCandidate(candidate);
  const following = editor.state.doc.textBetween(
    state.range.to,
    Math.min(state.range.to + 1, editor.state.doc.content.size),
    "\n",
    "\n",
  );
  const separator = following && !/\s/u.test(following) ? " " : "";
  return editor
    .chain()
    .focus()
    .deleteRange(state.range)
    .insertContent({
      type: "assistantMention",
      attrs: {
        kind: normalized.kind,
        value: normalized.value,
        label: normalized.label,
      },
    })
    .insertContent(separator)
    .run();
}

export const AssistantMentionExtension =
  Node.create<AssistantMentionExtensionOptions>({
    name: "assistantMention",

    addOptions() {
      return {
        getCandidates: () => [],
        enabled: () => true,
      };
    },

    group: "inline",
    inline: true,
    atom: true,
    selectable: true,

    addAttributes() {
      return {
        kind: { default: "file" },
        value: { default: "" },
        label: { default: "" },
      };
    },

    parseHTML() {
      // External clipboard HTML intentionally falls back to readable text and
      // cannot silently restore an authorization-bearing mention node.
      return [];
    },

    renderHTML({ node, HTMLAttributes }) {
      return [
        "span",
        mergeAttributes(
          {
            "data-assistant-mention": "true",
            "data-mention-kind": String(node.attrs.kind ?? "file"),
            contenteditable: "false",
          },
          HTMLAttributes,
        ),
        String(node.attrs.label ?? ""),
      ];
    },

    addNodeView() {
      return ReactNodeViewRenderer(AssistantMentionNodeView);
    },

    addProseMirrorPlugins() {
      const dismissedRanges: Record<
        "at" | "hash",
        { from: number; to: number } | null
      > = {
        at: null,
        hash: null,
      };
      const buildSuggestion = (char: "@" | "#", pluginKey: "at" | "hash") =>
        Suggestion<MentionCandidate>({
          editor: this.editor,
          char,
          allowSpaces: true,
          // The custom Unicode-aware boundary below is authoritative. TipTap's
          // ASCII prefix filter would otherwise reject Chinese fullwidth
          // brackets before `@` / `#`.
          allowedPrefixes: null,
          pluginKey:
            pluginKey === "at"
              ? assistantMentionPluginKeys.at
              : assistantMentionPluginKeys.hash,
          allow: ({ editor, range }) => {
            const dismissed = dismissedRanges[pluginKey];
            if (dismissed?.from === range.from && dismissed.to === range.to) {
              return false;
            }
            dismissedRanges[pluginKey] = null;
            return (
              (this.options.enabled?.() ?? true) &&
              isMentionBoundary(editor, range.from)
            );
          },
          items: ({ query }) => this.options.getCandidates(char, query),
          command: ({ editor, range, props }) => {
            const candidate = normalizeCandidate(props as MentionCandidate);
            const following = editor.state.doc.textBetween(
              range.to,
              Math.min(range.to + 1, editor.state.doc.content.size),
              "\n",
              "\n",
            );
            const separator = following && !/\s/u.test(following) ? " " : "";
            editor
              .chain()
              .focus()
              .deleteRange(range)
              .insertContent({
                type: "assistantMention",
                attrs: {
                  kind: candidate.kind,
                  value: candidate.value,
                  label: candidate.label,
                },
              })
              .insertContent(separator)
              .run();
          },
          render: () => {
            let component: ReactRenderer<AiMentionPopoverRef> | null = null;
            let popup: SuggestionPopup | null = null;

            return {
              onStart: (props) => {
                component = new ReactRenderer(AiMentionPopover, {
                  props: { ...props, prefix: char },
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
                  })[0] as unknown as SuggestionPopup;
                });
              },
              onUpdate(props) {
                component?.updateProps({ ...props, prefix: char });
                if (props.clientRect && popup) {
                  popup.setProps({
                    getReferenceClientRect: props.clientRect as () => DOMRect,
                  });
                }
              },
              onKeyDown(props) {
                if (props.event.key === "Escape") {
                  dismissedRanges[pluginKey] = { ...props.range };
                  popup?.destroy();
                  props.view.dispatch(props.view.state.tr);
                  return true;
                }
                return component?.ref?.onKeyDown(props.event) ?? false;
              },
              onExit() {
                popup?.destroy();
                component?.destroy();
                popup = null;
                component = null;
              },
            };
          },
        });

      return [buildSuggestion("@", "at"), buildSuggestion("#", "hash")];
    },
  });
