import { Folder, FileText, Hash, X } from "lucide-react";

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
    <div className={cn("flex flex-wrap gap-1.5 px-3 pt-2", className)}>
      {tokens.map((t) => (
        <span
          key={`${t.kind}:${t.value}`}
          className="inline-flex max-w-[220px] items-center gap-1 rounded-sm border border-border/45 bg-surface-elevated/35 px-1.5 py-0.5 text-[11px] leading-4 text-muted-foreground/85"
        >
          {t.kind === "folder" ? (
            <Folder className="h-3 w-3 shrink-0 text-muted-foreground/70" />
          ) : t.kind === "tag" ? (
            <Hash className="h-3 w-3 shrink-0 text-muted-foreground/70" />
          ) : (
            <FileText className="h-3 w-3 shrink-0 text-muted-foreground/70" />
          )}
          <span className="truncate">{t.label}</span>
          <button
            type="button"
            className="rounded-sm p-0.5 text-muted-foreground/60 hover:bg-muted hover:text-foreground"
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
