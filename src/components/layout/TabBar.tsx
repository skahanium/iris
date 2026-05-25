import { Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface TabItem {
  path: string;
  title: string;
  dirty?: boolean;
}

interface TabBarProps {
  tabs: TabItem[];
  activePath: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onNew: () => void;
}

export function TabBar({
  tabs,
  activePath,
  onSelect,
  onClose,
  onNew,
}: TabBarProps) {
  return (
    <div className="flex h-9 items-center gap-0.5 border-b border-border bg-panel/95 px-2">
      {tabs.map((tab) => (
        <button
          key={tab.path}
          type="button"
          className={cn(
            "flex max-w-[200px] items-center gap-1 rounded px-2 py-1 text-xs",
            activePath === tab.path
              ? "bg-muted/80 text-foreground"
              : "text-muted-foreground hover:bg-muted/40",
          )}
          onClick={() => onSelect(tab.path)}
        >
          <span className="truncate">
            {tab.dirty ? "• " : ""}
            {tab.title}
          </span>
          <X
            className="h-3 w-3 shrink-0 opacity-60 hover:opacity-100"
            onClick={(e) => {
              e.stopPropagation();
              onClose(tab.path);
            }}
          />
        </button>
      ))}
      <Button type="button" size="icon" variant="ghost" onClick={onNew}>
        <Plus className="h-4 w-4" />
      </Button>
    </div>
  );
}
