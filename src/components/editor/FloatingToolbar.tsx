import type { Editor } from "@tiptap/react";
import { ChevronDown, Sparkles } from "lucide-react";
import { memo, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface FloatingToolbarProps {
  editor: Editor | null;
  onInlineAi: (action: string) => void;
  onSendToAi: () => void;
}

const PRIMARY_ACTIONS = [
  { id: "rewrite", label: "改写" },
  { id: "expand", label: "扩写" },
  { id: "simplify", label: "简化" },
] as const;

const MORE_ACTIONS = [
  { id: "cite", label: "引用" },
  { id: "check", label: "检查" },
] as const;

export const FloatingToolbar = memo(function FloatingToolbar({
  editor,
  onInlineAi,
  onSendToAi,
}: FloatingToolbarProps) {
  const [moreOpen, setMoreOpen] = useState(false);

  if (!editor || editor.state.selection.empty) return null;

  const handleInlineAi = (action: string) => {
    if (!editor) return;

    const { from, to } = editor.state.selection;
    const selectedText = editor.state.doc.textBetween(from, to, " ");

    editor.commands.insertInlineAi({
      action: action as
        | "continue"
        | "rewrite"
        | "expand"
        | "simplify"
        | "cite"
        | "check",
      context: selectedText,
    });

    onInlineAi(action);
    setMoreOpen(false);
  };

  return (
    <div className="fixed bottom-20 left-1/2 z-toolbar flex -translate-x-1/2 items-center gap-1 rounded-lg border border-border/80 bg-surface-elevated/95 px-1.5 py-1 shadow-floating backdrop-blur-[2px]">
      <span className="flex h-7 w-7 shrink-0 items-center justify-center text-primary">
        <Sparkles className="h-3.5 w-3.5" aria-hidden />
      </span>
      <div
        className="flex items-center gap-0.5"
        role="group"
        aria-label="AI 润色"
      >
        {PRIMARY_ACTIONS.map((a) => (
          <button
            key={a.id}
            type="button"
            className="rounded-md px-2.5 py-1 text-xs font-medium text-foreground/90 transition-[background-color,transform] duration-fast ease-iris-out hover:bg-surface-inset active:scale-[0.98] motion-reduce:active:scale-100"
            onClick={() => handleInlineAi(a.id)}
          >
            {a.label}
          </button>
        ))}
        <div className="relative">
          <button
            type="button"
            className={cn(
              "inline-flex items-center gap-0.5 rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors duration-fast ease-iris-out hover:bg-surface-inset hover:text-foreground",
              moreOpen && "bg-surface-inset text-foreground",
            )}
            aria-expanded={moreOpen}
            aria-haspopup="menu"
            onClick={() => setMoreOpen((o) => !o)}
          >
            更多
            <ChevronDown className="h-3 w-3" />
          </button>
          {moreOpen ? (
            <div
              className="absolute bottom-full left-0 mb-1 min-w-[7rem] overflow-hidden rounded-md border border-border/80 bg-surface-elevated py-0.5 shadow-floating"
              role="menu"
            >
              {MORE_ACTIONS.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  role="menuitem"
                  className="flex w-full px-3 py-1.5 text-left text-xs hover:bg-surface-inset/80"
                  onClick={() => handleInlineAi(a.id)}
                >
                  {a.label}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>
      <span className="mx-0.5 h-5 w-px bg-border/80" aria-hidden />
      <Button
        type="button"
        size="sm"
        variant="secondary"
        className="h-7 text-xs"
        onClick={onSendToAi}
      >
        发送到 AI
      </Button>
    </div>
  );
});
