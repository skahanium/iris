import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function isModKey(e: KeyboardEvent): boolean {
  return e.ctrlKey || e.metaKey;
}

export interface DebouncedFn<T extends (...args: never[]) => void> {
  (...args: Parameters<T>): void;
  flush: () => void;
  cancel: () => void;
}

export function debounce<T extends (...args: never[]) => void>(
  fn: T,
  ms: number,
): DebouncedFn<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let lastArgs: Parameters<T> | undefined;

  const debounced = (...args: Parameters<T>) => {
    lastArgs = args;
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      lastArgs = undefined;
      fn(...args);
    }, ms);
  };

  debounced.flush = () => {
    if (timer !== undefined && lastArgs !== undefined) {
      clearTimeout(timer);
      timer = undefined;
      const args = lastArgs;
      lastArgs = undefined;
      fn(...args);
    }
  };

  debounced.cancel = () => {
    clearTimeout(timer);
    timer = undefined;
    lastArgs = undefined;
  };

  return debounced;
}
