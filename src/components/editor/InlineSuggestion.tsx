interface InlineSuggestionData {
  text: string;
  confidence: number;
  source: string;
}

interface InlineSuggestionProps {
  suggestion: InlineSuggestionData;
  onAccept: () => void;
  onDismiss: () => void;
}

export function InlineSuggestion({
  suggestion,
  onAccept,
  onDismiss,
}: InlineSuggestionProps) {
  return (
    <div className="absolute z-50 mt-1 max-w-md rounded-lg border bg-popover p-2 shadow-lg">
      <div className="flex items-start gap-2">
        <div className="flex-1">
          <p className="text-sm text-muted-foreground">{suggestion.text}</p>
          <p className="mt-1 text-[10px] text-muted-foreground/70">
            来源: {suggestion.source}
          </p>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onAccept}
            className="rounded bg-primary px-2 py-1 text-xs text-primary-foreground hover:bg-primary/90"
          >
            接受
          </button>
          <button
            type="button"
            onClick={onDismiss}
            className="rounded border px-2 py-1 text-xs hover:bg-muted"
          >
            忽略
          </button>
        </div>
      </div>
    </div>
  );
}
