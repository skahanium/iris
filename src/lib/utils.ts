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
  if (matchesPhysicalKeyCode(e.code, chord.key)) return true;
  return false;
}

function matchesPhysicalKeyCode(code: string, key: string): boolean {
  switch (key) {
    case ".":
      return code === "Period" || code === "NumpadDecimal";
    case ",":
      return code === "Comma";
    case "-":
      return code === "Minus" || code === "NumpadSubtract";
    case "+":
    case "=":
      return code === "Equal" || code === "NumpadAdd";
    default:
      return false;
  }
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
