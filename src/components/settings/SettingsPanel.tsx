import { Moon, Sun } from "lucide-react";

import { Button } from "@/components/ui/button";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { ScrollArea } from "@/components/ui/scroll-area";

interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
}

export function SettingsPanel({
  open,
  onClose,
  theme,
  onThemeChange,
}: SettingsPanelProps) {
  return (
    <IrisOverlay open={open} onClose={onClose} title="设置" size="command">
      <ScrollArea className="flex-1">
        <div className="space-y-6 px-4 py-4">
          <section>
            <h3 className="mb-2 text-xs font-medium text-foreground">外观</h3>
            <div className="flex gap-2">
              <Button
                type="button"
                size="sm"
                variant={theme === "dark" ? "default" : "outline"}
                className="gap-1.5"
                onClick={() => onThemeChange("dark")}
              >
                <Moon className="h-3.5 w-3.5" />
                暗色
              </Button>
              <Button
                type="button"
                size="sm"
                variant={theme === "light" ? "default" : "outline"}
                className="gap-1.5"
                onClick={() => onThemeChange("light")}
              >
                <Sun className="h-3.5 w-3.5" />
                亮色
              </Button>
            </div>
          </section>

          <section data-testid="settings-section-about">
            <h3 className="mb-2 text-xs font-medium text-foreground">
              关于 Iris
            </h3>
            <div className="rounded-md border border-border/70 bg-surface-inset/40 px-3 py-2 text-xs leading-5 text-muted-foreground">
              <div className="font-medium text-foreground">Iris</div>
              <div>版本 1.0.0</div>
              <div>Copyright (C) 2026 Iris Contributors</div>
              <div>Licensed under GNU Affero General Public License v3.0</div>
              <div>
                License: <span className="font-mono">LICENSE</span>
                <span className="px-1 text-muted-foreground/60">·</span>
                Source:{" "}
                <span className="font-mono">
                  https://github.com/skahanium/iris
                </span>
              </div>
            </div>
          </section>
        </div>
      </ScrollArea>
    </IrisOverlay>
  );
}
