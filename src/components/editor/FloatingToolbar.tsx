import type { Editor } from "@tiptap/react";
import { Sparkles } from "lucide-react";
import { memo } from "react";

import { Button } from "@/components/ui/button";

interface FloatingToolbarProps {
  editor: Editor | null;
  onInlineAi: (action: string) => void;
  onSendToAi: () => void;
}

export const FloatingToolbar = memo(function FloatingToolbar({
  editor,
  onInlineAi,
  onSendToAi,
}: FloatingToolbarProps) {
  if (!editor || editor.state.selection.empty) return null;

  const actions = [
    { id: "rewrite", label: "改写" },
    { id: "expand", label: "扩写" },
    { id: "translate", label: "翻译" },
    { id: "simplify", label: "简化" },
  ];

  return (
    <div className="fixed bottom-20 left-1/2 z-30 flex -translate-x-1/2 gap-1 rounded-lg border border-border bg-panel/95 px-2 py-1.5 shadow-floating backdrop-blur-sm">
      {actions.map((a) => (
        <Button
          key={a.id}
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => onInlineAi(a.id)}
        >
          <Sparkles className="mr-1 h-3 w-3" />
          {a.label}
        </Button>
      ))}
      <Button type="button" size="sm" variant="outline" onClick={onSendToAi}>
        发送到 AI
      </Button>
    </div>
  );
});
