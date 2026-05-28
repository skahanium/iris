import { Folder, FileText, X } from "lucide-react";

import type { MentionToken } from "@/lib/ai-context-scope";
import { cn } from "@/lib/utils";

interface ContextScopeChipsProps {
  tokens: MentionToken[];
  onRemove: (token: MentionToken) => void;
  className?: string;
}

export function ContextScopeChips({
  tokens,
  onRemove,
  className,
}: ContextScopeChipsProps) {
  if (tokens.length === 0) return null;

  return (
    <div className={cn("flex flex-wrap gap-1 px-3 pt-2", className)}>
      {tokens.map((t) => (
        <span
          key={`${t.kind}:${t.value}`}
          className="inline-flex max-w-[200px] items-center gap-1 rounded-md border border-border/80 bg-secondary/60 px-2 py-0.5 text-[10px] text-foreground"
        >
          {t.kind === "folder" ? (
            <Folder className="h-3 w-3 shrink-0 text-muted-foreground" />
          ) : (
            <FileText className="h-3 w-3 shrink-0 text-muted-foreground" />
          )}
          <span className="truncate">{t.label}</span>
          <button
            type="button"
            className="rounded p-0.5 hover:bg-muted"
            aria-label={`移除 ${t.label}`}
            onClick={() => onRemove(t)}
          >
            <X className="h-3 w-3" />
          </button>
        </span>
      ))}
    </div>
  );
}
