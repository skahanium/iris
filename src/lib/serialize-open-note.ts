import type { Editor } from "@tiptap/react";

import { exportEditorToMarkdown } from "@/lib/editor-export";
import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { splitFrontmatter } from "@/lib/frontmatter";
import { buildNoteMarkdown } from "@/lib/markdown";
import { debugSessionLog } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";

export interface SerializeOpenNoteOptions {
  yaml: string | null;
  title: string;
  editor: Editor | null;
  /** Used when `editor` is unavailable; typically `splitFrontmatter(ref).body`. */
  bodyFallbackMd: string;
  /** Layer-1 edits since last successful save; gates HTML fallback (avoids open-time bloat). */
  isDirty?: boolean;
  /** `doc.textContent.length` captured when the note body was loaded into the editor. */
  baselineDocChars?: number;
}

export type SerializeBodySource = "pm" | "html" | "fallback";

/** Spacer-aware HTML export (plain turndown turns `iris-spacer` into extra blank lines). */
function spacerAwareHtmlBody(editor: Editor): string {
  return exportEditorToMarkdown({
    editorHtml: editor.getHTML(),
    originalMarkdown: "",
    classifiedFragments: [],
  }).bodyMarkdown;
}

/**
 * PM is the default save path. Spacer-aware HTML only when the tab is dirty, PM
 * did not grow vs ref, and the live doc grew since open (v8: htmlMuchLarger on
 * idle save bloated ref and broke tail edits).
 */
function pickBodyMarkdown(
  editor: Editor,
  bodyFallbackMd: string,
  ctx: { isDirty: boolean; baselineDocChars: number },
): {
  bodyMd: string;
  source: SerializeBodySource;
  pmLen: number;
  htmlLen: number;
  docChars: number;
  pickReason: string;
} {
  const pmMd = editorDocToMarkdown(editor);
  const htmlMd = spacerAwareHtmlBody(editor);
  const docChars = editor.state.doc.textContent.length;
  const { isDirty, baselineDocChars } = ctx;

  if (!isDirty) {
    return {
      bodyMd: pmMd,
      source: "pm",
      pmLen: pmMd.length,
      htmlLen: htmlMd.length,
      docChars,
      pickReason: "clean-pm",
    };
  }

  const pmMatchesRef = pmMd === bodyFallbackMd;
  const pmGrew = pmMd.length > bodyFallbackMd.length + 2;
  const docGrewSinceOpen =
    baselineDocChars > 0 && docChars > baselineDocChars;
  const useHtml =
    !pmGrew &&
    pmMatchesRef &&
    docGrewSinceOpen &&
    htmlMd.length > pmMd.length + 12;

  return {
    bodyMd: useHtml ? htmlMd : pmMd,
    source: useHtml ? "html" : "pm",
    pmLen: pmMd.length,
    htmlLen: htmlMd.length,
    docChars,
    pickReason: useHtml
      ? "dirty-stale-pm-html"
      : pmGrew
        ? "dirty-pm-grew"
        : "dirty-pm-default",
  };
}

/** Single persistence pipeline: title state + TipTap body → full note markdown. */
export function serializeOpenNote(options: SerializeOpenNoteOptions): string {
  const {
    yaml,
    title,
    editor,
    bodyFallbackMd,
    isDirty = false,
    baselineDocChars = 0,
  } = options;
  let bodyMd = bodyFallbackMd;
  let source: SerializeBodySource = "fallback";
  let pmLen = 0;
  let htmlLen = 0;
  let docChars = 0;
  let pickReason = "fallback";
  const hasEditor = editor != null && !editor.isDestroyed;

  if (hasEditor) {
    const picked = pickBodyMarkdown(editor, bodyFallbackMd, {
      isDirty,
      baselineDocChars,
    });
    bodyMd = picked.bodyMd;
    source = picked.source;
    pmLen = picked.pmLen;
    htmlLen = picked.htmlLen;
    docChars = picked.docChars;
    pickReason = picked.pickReason;
  }

  const md = buildNoteMarkdown(yaml, title.trim(), bodyMd);

  if (isTauriRuntime()) {
    void debugSessionLog({
      location: "serialize-open-note.ts",
      message: "serializeOpenNote",
      hypothesisId: "H3",
      runId: "post-fix-v9",
      data: {
        hasEditor,
        isDirty,
        baselineDocChars,
        pickReason,
        source,
        useHtml: source === "html",
        mdLen: md.length,
        refLen: bodyFallbackMd.length,
        pmLen,
        htmlLen,
        docChars,
      },
    });
  }

  return md;
}

/** Body markdown from a persisted note ref (for fallbacks). */
export function bodyMarkdownFromNoteRef(noteMarkdown: string): string {
  return splitFrontmatter(noteMarkdown).body;
}
