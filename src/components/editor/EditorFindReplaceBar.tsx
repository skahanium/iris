import type { Editor } from "@tiptap/react";
import { ChevronDown, ChevronUp, Replace, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { findTextRangesInDoc, type TextRange } from "@/lib/editor-find-replace";

import { setFindHighlightState } from "./extensions/FindHighlightExtension";

export interface EditorFindReplaceBarProps {
  editor: Editor | null;
  mode: "find" | "replace";
  open: boolean;
  onClose: () => void;
  onModeChange?: (mode: "find" | "replace") => void;
}

function currentRange(
  ranges: TextRange[],
  currentIndex: number,
): TextRange | null {
  if (ranges.length === 0) return null;
  return ranges[Math.min(Math.max(currentIndex, 0), ranges.length - 1)] ?? null;
}

export function EditorFindReplaceBar({
  editor,
  mode,
  open,
  onClose,
  onModeChange,
}: EditorFindReplaceBarProps) {
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [currentIndex, setCurrentIndex] = useState(0);

  const doc = editor?.state.doc;

  const ranges = useMemo(() => {
    if (!doc || !query) return [];
    return findTextRangesInDoc(doc, query, { caseSensitive });
  }, [caseSensitive, doc, query]);

  const boundedIndex =
    ranges.length === 0 ? 0 : Math.min(currentIndex, ranges.length - 1);

  useEffect(() => {
    if (!editor) return;
    if (!open) {
      setFindHighlightState(editor, {
        query: "",
        caseSensitive: false,
        currentIndex: 0,
      });
      return;
    }
    setFindHighlightState(editor, {
      query,
      caseSensitive,
      currentIndex: boundedIndex,
    });
  }, [boundedIndex, caseSensitive, editor, open, query, ranges.length]);

  const selectRange = useCallback(
    (range: TextRange | null) => {
      if (!editor || !range) return;
      editor.commands.setTextSelection({ from: range.from, to: range.to });
    },
    [editor],
  );

  useEffect(() => {
    selectRange(currentRange(ranges, boundedIndex));
  }, [boundedIndex, ranges, selectRange]);

  const go = useCallback(
    (direction: 1 | -1) => {
      if (ranges.length === 0) return;
      setCurrentIndex(
        (index) => (index + direction + ranges.length) % ranges.length,
      );
    },
    [ranges.length],
  );

  const replaceCurrent = useCallback(() => {
    if (!editor) return;
    const range = currentRange(ranges, boundedIndex);
    if (!range) return;
    editor.view.dispatch(
      editor.state.tr.insertText(replacement, range.from, range.to),
    );
    editor.commands.focus();
  }, [boundedIndex, editor, ranges, replacement]);

  const replaceAll = useCallback(() => {
    if (!editor || ranges.length === 0) return;
    const tr = editor.state.tr;
    for (const range of [...ranges].sort((a, b) => b.from - a.from)) {
      tr.insertText(replacement, range.from, range.to);
    }
    editor.view.dispatch(tr);
    editor.commands.focus();
    setCurrentIndex(0);
  }, [editor, ranges, replacement]);

  if (!open || !editor) return null;

  const matchText =
    ranges.length === 0 ? "0 / 0" : `${boundedIndex + 1} / ${ranges.length}`;

  return (
    <div
      className="iris-find-replace-bar editor-edge-control"
      role="search"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose();
        }
        if (event.key === "Enter") {
          event.preventDefault();
          go(event.shiftKey ? -1 : 1);
        }
      }}
    >
      <Search className="h-4 w-4 text-muted-foreground" aria-hidden />
      <Input
        aria-label="查找"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setCurrentIndex(0);
        }}
        placeholder="查找"
        className="h-8 w-48"
      />
      <span className="min-w-12 text-center text-xs tabular-nums text-muted-foreground">
        {matchText}
      </span>
      <Button type="button" size="icon" variant="ghost" onClick={() => go(-1)}>
        <ChevronUp className="h-4 w-4" />
      </Button>
      <Button type="button" size="icon" variant="ghost" onClick={() => go(1)}>
        <ChevronDown className="h-4 w-4" />
      </Button>
      <label className="flex items-center gap-1 text-xs text-muted-foreground">
        <input
          type="checkbox"
          checked={caseSensitive}
          onChange={(event) => setCaseSensitive(event.target.checked)}
        />
        Aa
      </label>
      {mode === "replace" ? (
        <>
          <Replace className="h-4 w-4 text-muted-foreground" aria-hidden />
          <Input
            aria-label="替换为"
            value={replacement}
            onChange={(event) => setReplacement(event.target.value)}
            placeholder="替换为"
            className="h-8 w-48"
          />
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={replaceCurrent}
          >
            替换
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            data-testid="replace-all"
            onClick={replaceAll}
          >
            全部
          </Button>
        </>
      ) : (
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => onModeChange?.("replace")}
        >
          替换
        </Button>
      )}
      <Button type="button" size="icon" variant="ghost" onClick={onClose}>
        <X className="h-4 w-4" />
      </Button>
    </div>
  );
}
