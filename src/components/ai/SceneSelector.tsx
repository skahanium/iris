import { Check, ChevronDown, BookOpen, FlaskConical, PenLine, Search } from "lucide-react";
import { useState, useRef, useEffect } from "react";

import type { AiScene } from "@/types/ai";
import { SCENE_OPTIONS } from "@/lib/ai/scene-types";

const SCENE_ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  Search,
  BookOpen,
  PenLine,
  FlaskConical,
};

interface SceneSelectorProps {
  scene: AiScene;
  onSceneChange: (scene: AiScene) => void;
}

export function SceneSelector({ scene, onSceneChange }: SceneSelectorProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  const current = SCENE_OPTIONS.find((s) => s.scene === scene) ?? SCENE_OPTIONS[0]!;
  const Icon = SCENE_ICONS[current.icon] ?? Search;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs font-medium text-muted-foreground hover:bg-muted/50 hover:text-foreground transition-colors"
      >
        <Icon className="h-3.5 w-3.5" />
        {current.label}
        <ChevronDown className="h-3 w-3 opacity-50" />
      </button>

      {open && (
        <div className="absolute left-0 top-full z-50 mt-1 w-48 rounded-md border border-border bg-panel p-1 shadow-lg">
          {SCENE_OPTIONS.map((opt) => {
            const OptIcon = SCENE_ICONS[opt.icon] ?? Search;
            return (
              <button
                key={opt.scene}
                type="button"
                onClick={() => {
                  onSceneChange(opt.scene);
                  setOpen(false);
                }}
                className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-xs hover:bg-muted/50 transition-colors"
              >
                <OptIcon className="h-3.5 w-3.5 text-muted-foreground" />
                <div className="flex-1 text-left">
                  <div className="font-medium">{opt.label}</div>
                  <div className="text-[10px] text-muted-foreground/70">{opt.description}</div>
                </div>
                {opt.scene === scene && <Check className="h-3 w-3 text-primary" />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
