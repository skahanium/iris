import { Extension, type Editor } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

import { findTextRangesInDoc, type TextRange } from "@/lib/editor-find-replace";

export interface FindHighlightInput {
  query: string;
  caseSensitive: boolean;
  currentIndex: number;
}

export interface FindHighlightState extends FindHighlightInput {
  ranges: TextRange[];
  decorations: DecorationSet;
}

export const findHighlightPluginKey = new PluginKey<FindHighlightState>(
  "irisFindHighlight",
);

function clampCurrentIndex(index: number, ranges: TextRange[]): number {
  if (ranges.length === 0) {
    return 0;
  }
  return Math.min(Math.max(index, 0), ranges.length - 1);
}

function buildState(
  doc: ProseMirrorNode,
  input: FindHighlightInput,
): FindHighlightState {
  const ranges = findTextRangesInDoc(doc, input.query, {
    caseSensitive: input.caseSensitive,
  });
  const currentIndex = clampCurrentIndex(input.currentIndex, ranges);
  const decorations = DecorationSet.create(
    doc,
    ranges.map((range, index) =>
      Decoration.inline(range.from, range.to, {
        class:
          index === currentIndex
            ? "iris-find-match iris-find-match-current"
            : "iris-find-match",
      }),
    ),
  );
  return {
    ...input,
    currentIndex,
    ranges,
    decorations,
  };
}

function emptyState(doc: ProseMirrorNode): FindHighlightState {
  return buildState(doc, {
    query: "",
    caseSensitive: false,
    currentIndex: 0,
  });
}

export function setFindHighlightState(
  editor: Editor,
  input: FindHighlightInput,
): void {
  editor.view.dispatch(editor.state.tr.setMeta(findHighlightPluginKey, input));
}

export const FindHighlightExtension = Extension.create({
  name: "findHighlight",

  addProseMirrorPlugins() {
    return [
      new Plugin<FindHighlightState>({
        key: findHighlightPluginKey,
        state: {
          init: (_config, state) => emptyState(state.doc),
          apply: (tr, previous, _oldState, newState) => {
            const meta = tr.getMeta(findHighlightPluginKey) as
              | FindHighlightInput
              | undefined;
            if (meta) {
              return buildState(newState.doc, meta);
            }
            if (tr.docChanged && previous.query) {
              return buildState(newState.doc, previous);
            }
            if (!tr.docChanged) {
              return previous;
            }
            return emptyState(newState.doc);
          },
        },
        props: {
          decorations(state) {
            return findHighlightPluginKey.getState(state)?.decorations;
          },
        },
      }),
    ];
  },
});
