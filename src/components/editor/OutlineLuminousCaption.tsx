import type { OutlineEntry } from "@/lib/document-outline";
import { cn } from "@/lib/utils";

interface OutlineLuminousCaptionProps {
  entry: OutlineEntry;
  variant: "hover" | "active" | "focus";
}

/** Anchored to parent tick; never positioned on the track directly. */
export function OutlineLuminousCaption({
  entry,
  variant,
}: OutlineLuminousCaptionProps) {
  return (
    <span
      role="tooltip"
      data-testid="outline-luminous-caption"
      className={cn(
        "outline-luminous-caption",
        `outline-luminous-caption--level-${entry.level}`,
        `outline-luminous-caption--${variant}`,
      )}
    >
      <span className="truncate" title={entry.text}>
        {entry.text}
      </span>
    </span>
  );
}
