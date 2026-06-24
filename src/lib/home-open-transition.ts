interface CurrentRef<T> {
  current: T;
}

export type HomePendingOpenKind = "note" | "new-note";

export interface HomePendingOpen {
  error?: string;
  kind: HomePendingOpenKind;
  path: string | null;
  sequence: number;
  title: string;
}

interface BeginHomeOpenLoadingOptions {
  kind?: HomePendingOpenKind;
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

export function beginHomeOpenLoading({
  kind = "note",
  path,
  sequenceRef,
  setPendingOpen,
  title,
}: BeginHomeOpenLoadingOptions): number {
  sequenceRef.current += 1;
  setPendingOpen({
    kind,
    path,
    sequence: sequenceRef.current,
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
