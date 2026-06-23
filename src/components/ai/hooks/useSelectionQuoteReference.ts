import { useEffect } from "react";

import { createContextReference } from "@/lib/context-reference";
import type { ContextReference } from "@/types/ai";

import type { AssistantSelectionQuote } from "../types";

interface UseSelectionQuoteReferenceParams {
  quoteSelectionAsReference: (reference: ContextReference) => void;
  selectionQuote?: AssistantSelectionQuote | null;
}

export function useSelectionQuoteReference({
  quoteSelectionAsReference,
  selectionQuote,
}: UseSelectionQuoteReferenceParams) {
  const text = selectionQuote?.text ?? "";
  const content = selectionQuote?.content ?? text;
  const filePath = selectionQuote?.filePath ?? null;
  const editorFrom = selectionQuote?.editorRange?.from ?? null;
  const editorTo = selectionQuote?.editorRange?.to ?? null;

  useEffect(() => {
    if (!text || !filePath) return;
    quoteSelectionAsReference(
      createContextReference({
        kind: "selection",
        filePath,
        content,
        excerpt: text,
        utf8Range: null,
        editorRange:
          editorFrom === null || editorTo === null
            ? null
            : { from: editorFrom, to: editorTo },
      }),
    );
  }, [
    content,
    editorFrom,
    editorTo,
    filePath,
    quoteSelectionAsReference,
    text,
  ]);
}
