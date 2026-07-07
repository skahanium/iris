interface CurrentRef<T> {
  current: T;
}

export type HomePendingOpenKind = "note" | "new-note";
export type HomePendingOpenLoadingPolicy = "defer" | "disabled";

export interface HomePendingOpen {
  error?: string;
  kind: HomePendingOpenKind;
  loadingPolicy: HomePendingOpenLoadingPolicy;
  path: string | null;
  sequence: number;
  startedAt: number;
  title: string;
}

interface BeginHomeOpenLoadingOptions {
  kind?: HomePendingOpenKind;
  loadingPolicy?: HomePendingOpenLoadingPolicy;
  path: string | null;
  sequenceRef: CurrentRef<number>;
  setPendingOpen: (pending: HomePendingOpen | null) => void;
  title: string;
}

interface ClearHomeOpenLoadingOptions {
  activePath: string | null;
  path: string;
  sequence: number;
  sequenceRef: CurrentRef<number>;
  setPendingOpen: (pending: HomePendingOpen | null) => void;
}

interface ClearHomeNewNoteLoadingOptions {
  activePath: string | null;
  previousPath: string | null;
  sequence: number;
  sequenceRef: CurrentRef<number>;
  setPendingOpen: (pending: HomePendingOpen | null) => void;
}

interface FailHomeOpenLoadingOptions {
  message: string;
  pending: HomePendingOpen;
  sequence: number;
  sequenceRef: CurrentRef<number>;
  setPendingOpen: (pending: HomePendingOpen | null) => void;
}

function openTransitionNow(): number {
  return globalThis.performance?.now?.() ?? Date.now();
}

export function beginHomeOpenLoading({
  kind = "note",
  loadingPolicy,
  path,
  sequenceRef,
  setPendingOpen,
  title,
}: BeginHomeOpenLoadingOptions): number {
  sequenceRef.current += 1;
  setPendingOpen({
    kind,
    loadingPolicy: loadingPolicy ?? "defer",
    path,
    sequence: sequenceRef.current,
    startedAt: openTransitionNow(),
    title,
  });
  return sequenceRef.current;
}

export function cancelHomeOpenTransitions(
  sequenceRef: CurrentRef<number>,
  setPendingOpen?: (pending: HomePendingOpen | null) => void,
): void {
  sequenceRef.current += 1;
  setPendingOpen?.(null);
}

export function clearHomeOpenLoading({
  activePath,
  path,
  sequence,
  sequenceRef,
  setPendingOpen,
}: ClearHomeOpenLoadingOptions): boolean {
  if (sequenceRef.current !== sequence || activePath !== path) {
    return false;
  }
  setPendingOpen(null);
  return true;
}

export function clearHomeNewNoteLoading({
  activePath,
  previousPath,
  sequence,
  sequenceRef,
  setPendingOpen,
}: ClearHomeNewNoteLoadingOptions): boolean {
  if (
    sequenceRef.current !== sequence ||
    activePath === null ||
    activePath === previousPath
  ) {
    return false;
  }
  setPendingOpen(null);
  return true;
}

export function failHomeOpenLoading({
  message,
  pending,
  sequence,
  sequenceRef,
  setPendingOpen,
}: FailHomeOpenLoadingOptions): boolean {
  if (sequenceRef.current !== sequence) {
    return false;
  }
  setPendingOpen({ ...pending, error: message });
  return true;
}
