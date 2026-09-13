/** Type surface for `file-size-budget.mjs`, which the Vitest gate imports. */

export declare const DEFAULT_MAX_LINES: number;
export declare const SCANNED_ROOTS: string[];
export declare const SPLIT_QUEUE: Record<string, number>;

export interface SourceFileSize {
  path: string;
  lines: number;
}

export interface BudgetViolation {
  path: string;
  lines: number;
  max: number;
  pinned: boolean;
}

export interface StalePin {
  path: string;
  reason: string;
}

export declare function countLines(content: string): number;
export declare function evaluateBudgets(
  entries: SourceFileSize[],
  options?: { defaultMax?: number; pinned?: Record<string, number> },
): { violations: BudgetViolation[]; stalePins: StalePin[] };
export declare function collectSourceFiles(root?: string): SourceFileSize[];
