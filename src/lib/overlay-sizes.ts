import { cn } from "@/lib/utils";

export type IrisOverlaySize =
  | "compact"
  | "palette"
  | "command"
  | "management"
  | "wide"
  | "near-full"
  | "graph";

export const IRIS_OVERLAY_SIZE_CLASS: Record<IrisOverlaySize, string> = {
  compact: "w-[calc(100vw-2rem)] max-w-xl",
  palette: "w-[min(640px,calc(100vw-2rem))] max-w-2xl",
  command: "h-[78vh] w-[80vw] max-w-3xl",
  management:
    "h-[min(760px,calc(100dvh-96px))] w-[min(1180px,calc(100vw-96px))] max-w-none",
  wide: "h-[88vh] w-[92vw] max-w-7xl",
  "near-full": "h-[88vh] w-[92vw] max-w-7xl",
  graph: "h-[92vh] w-[96vw] max-w-none",
};

/** Shared shell for task-capsule overlays; pair with `.task-overlay` surface tokens in globals.css. */
const IRIS_OVERLAY_PANEL_SHELL =
  "fixed left-1/2 top-1/2 z-overlay flex max-h-[calc(100dvh-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-xl border border-border bg-panel text-foreground shadow-overlay outline-none duration-base ease-iris-out data-[state=open]:animate-iris-enter data-[state=closed]:animate-iris-exit motion-reduce:data-[state=open]:animate-none motion-reduce:data-[state=closed]:animate-none";

export function irisOverlayPanelClass(
  size: IrisOverlaySize,
  className?: string,
): string {
  return cn(IRIS_OVERLAY_PANEL_SHELL, IRIS_OVERLAY_SIZE_CLASS[size], className);
}
