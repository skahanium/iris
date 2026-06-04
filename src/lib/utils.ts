import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function isModKey(e: KeyboardEvent): boolean {
  return e.ctrlKey || e.metaKey;
}

export interface KeyChord {
  key: string;
  mod: boolean;
  shift?: boolean;
  requireNote?: boolean;
  requireVault?: boolean;
  /** 引导键标识符（如 ⌘K）：按下后进入等待态，不直接分发 */
  leader?: string;
  /** 仅在指定引导键后被激活 */
  afterLeader?: string;
}

export function matchesKeyChord(e: KeyboardEvent, chord: KeyChord): boolean {
  if (chord.mod !== isModKey(e)) return false;
  if ((chord.shift ?? false) !== e.shiftKey) return false;
  const pressed = e.key;
  if (
    pressed === chord.key ||
    pressed.toLowerCase() === chord.key.toLowerCase()
  )
    return true;
  // Zoom in: same physical key produces + or = depending on layout
  if (chord.key === "+" && pressed === "=") return true;
  if (chord.key === "=" && pressed === "+") return true;
  return false;
}

export function isMacPlatform(): boolean {
  return (
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad|iPod/.test(navigator.platform)
  );
}

export function formatShortcut(chord: KeyChord): string {
  const isMac = isMacPlatform();
  const parts: string[] = [];
  if (chord.mod) parts.push(isMac ? "⌘" : "Ctrl");
  if (chord.shift) parts.push(isMac ? "⇧" : "Shift");
  parts.push(chord.key);
  return parts.join(isMac ? "" : "+");
}

/** Leader 第二键展示，如 ⌘K W */
export function formatLeaderChordShortcut(
  leaderKey: string,
  secondKey: string,
): string {
  const isMac = isMacPlatform();
  const leader = isMac ? `⌘${leaderKey}` : `Ctrl+${leaderKey}`;
  return `${leader} ${secondKey.toUpperCase()}`;
}

/** 状态栏等处的命令面板快捷键展示 */
export function formatCommandPaletteShortcut(): string {
  return formatShortcut({ key: "P", mod: true, shift: true });
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
