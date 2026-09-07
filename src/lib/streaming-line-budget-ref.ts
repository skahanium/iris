import { createContext, useContext, type MutableRefObject } from "react";

import type { StreamingLineBudget } from "@/lib/streaming-line-fit";

export const StreamingLineBudgetRefContext =
  createContext<MutableRefObject<StreamingLineBudget | null> | null>(null);

export function useStreamingLineBudgetRef(): MutableRefObject<StreamingLineBudget | null> | null {
  return useContext(StreamingLineBudgetRefContext);
}
