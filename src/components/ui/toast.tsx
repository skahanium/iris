import type { ReactNode } from "react";

import { useToastMessages } from "@/components/ui/use-toast";
import { cn } from "@/lib/utils";

export function ToastProvider({ children }: { children: ReactNode }) {
  const messages = useToastMessages();

  return (
    <>
      {children}
      <div
        className="pointer-events-none fixed bottom-8 left-1/2 z-overlay flex w-[min(22rem,calc(100vw-2rem))] -translate-x-1/2 flex-col items-center gap-2"
        aria-live="polite"
        aria-atomic="true"
      >
        {messages.map((toast) => (
          <div
            key={toast.id}
            className={cn(
              "pointer-events-auto rounded-md border border-border/70 bg-surface-elevated px-3 py-2 text-xs text-foreground shadow-overlay",
              toast.tone === "success" &&
                "border-[hsl(var(--status-llm-ready)/0.55)]",
              toast.tone === "error" &&
                "border-destructive/60 text-destructive",
            )}
            role="status"
          >
            {toast.message}
          </div>
        ))}
      </div>
    </>
  );
}
