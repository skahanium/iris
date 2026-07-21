import type { Editor } from "@tiptap/react";
import {
  memo,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

import { DocumentTitleContextMenu } from "@/components/editor/DocumentTitleContextMenu";

import {
  NOTE_TITLE_HARD_LIMIT,
  NOTE_TITLE_SOFT_LIMIT,
  sanitizeDocumentTitleInput,
} from "@/lib/note-title-limits";
import { cn } from "@/lib/utils";

interface DocumentTitleFieldProps {
  value: string;
  /** Remount the textarea when the open note changes (typically activePath). */
  resetKey: string;
  onChange: (value: string) => void;
  onBlur?: (committedTitle: string) => void;
  onCancel?: () => void;
  /** Notify parent when focus enters/leaves so sync effects can pause. */
  onFocusChange?: (focused: boolean) => void;
  editorRef: RefObject<Editor | null>;
  disabled?: boolean;
  readOnly?: boolean;
  placeholder?: string;
  className?: string;
}

function normalizeTitle(raw: string): string {
  return sanitizeDocumentTitleInput(raw).slice(0, NOTE_TITLE_HARD_LIMIT);
}

/**
 * Document title uses an uncontrolled textarea seeded from props.
 * While focused, the DOM is the source of truth so platform WebDriver /
 * IME / paste can mutate the field without fighting a React `value` prop.
 * Parent state and filename commit on blur (and context-menu edits).
 * When blurred, external `value` updates are mirrored into the DOM.
 *
 * Do not call setState in onFocus / during the click caret gesture: a React
 * re-render mid-mouseup resets WKWebView selection to 0 (caret jumps to start).
 * Selection is captured from DOM events (`onSelect` / `onInput`) — never from the
 * render body (React 19 may interrupt/retry render) — and restored in
 * useLayoutEffect after ancestor re-renders (autosave, tab dirty, staging).
 */
function DocumentTitleFieldInner({
  value,
  resetKey,
  onChange,
  onBlur,
  onCancel,
  onFocusChange,
  editorRef,
  disabled = false,
  readOnly = false,
  placeholder = "未命名文档",
  className,
}: DocumentTitleFieldProps) {
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const focusedRef = useRef(false);
  const cancelledRef = useRef(false);
  const pendingSelectionRef = useRef<{ start: number; end: number } | null>(
    null,
  );
  const [liveLen, setLiveLen] = useState(value.length);
  const showCount = liveLen > NOTE_TITLE_SOFT_LIMIT;

  const rememberSelection = useCallback(() => {
    const el = inputRef.current;
    if (!el || document.activeElement !== el) return;
    pendingSelectionRef.current = {
      start: el.selectionStart,
      end: el.selectionEnd,
    };
  }, []);

  const restorePendingSelection = useCallback(() => {
    const pending = pendingSelectionRef.current;
    if (!pending || !focusedRef.current) return;
    const el = inputRef.current;
    if (!el || document.activeElement !== el) return;
    try {
      el.setSelectionRange(pending.start, pending.end);
    } catch {
      // Ignore if the element is no longer selection-capable.
    }
    // Keep `pending` so later ancestor re-renders can restore again until blur.
  }, []);

  const resizeTitle = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    const selStart = el.selectionStart;
    const selEnd = el.selectionEnd;
    const active = document.activeElement === el;
    const prevHeight = el.style.height;
    const nextHeight = `${el.scrollHeight}px`;
    if (prevHeight === nextHeight) {
      return;
    }

    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
    if (active) {
      try {
        el.setSelectionRange(selStart, selEnd);
      } catch {
        // Ignore if the element is no longer selection-capable.
      }
    }
  }, []);

  useLayoutEffect(() => {
    if (focusedRef.current) return;
    const el = inputRef.current;
    if (!el) return;
    if (el.value !== value) {
      el.value = value;
    }
    setLiveLen(value.length);
  }, [value, resetKey]);

  useLayoutEffect(() => {
    resizeTitle();
  }, [resizeTitle, value, resetKey]);

  // Restore caret after ANY focused re-render (ancestor state or liveLen).
  // WKWebView resets selection to 0 on React commit while the textarea is focused.
  useLayoutEffect(() => {
    if (!focusedRef.current) return;
    restorePendingSelection();
  });

  useLayoutEffect(() => {
    if (!readOnly) return;
    const el = inputRef.current;
    if (!el || document.activeElement !== el) return;
    el.blur();
  }, [readOnly]);

  const commitFromDom = (raw: string) => {
    const next = normalizeTitle(raw);
    const el = inputRef.current;
    if (el && el.value !== next) {
      el.value = next;
    }
    setLiveLen(next.length);
    if (next !== value) {
      onChange(next);
    }
    return next;
  };

  const applyMenuValue = (raw: string) => {
    const next = normalizeTitle(raw);
    const el = inputRef.current;
    if (el) {
      el.value = next;
    }
    rememberSelection();
    setLiveLen(next.length);
    if (next !== value) {
      onChange(next);
    }
  };

  return (
    <DocumentTitleContextMenu
      inputRef={inputRef}
      onValueChange={applyMenuValue}
      readOnly={readOnly || disabled}
    >
      <div
        className={cn(
          "iris-document-title-field iris-doc-title-wrap",
          className,
        )}
        data-testid="document-title-field"
      >
        <textarea
          ref={inputRef}
          rows={1}
          data-testid="document-title"
          className="iris-doc-title"
          defaultValue={value}
          disabled={disabled}
          readOnly={readOnly}
          placeholder={placeholder}
          aria-label="文档标题"
          title={value || undefined}
          onInput={(event) => {
            const el = event.currentTarget;
            const next = normalizeTitle(el.value);
            rememberSelection();
            setLiveLen(next.length);
            resizeTitle();
            if (next !== value) {
              onChange(next);
            }
          }}
          onSelect={() => {
            rememberSelection();
          }}
          onFocus={() => {
            focusedRef.current = true;
            rememberSelection();
            onFocusChange?.(true);
          }}
          onBlur={(event) => {
            focusedRef.current = false;
            pendingSelectionRef.current = null;
            onFocusChange?.(false);
            if (cancelledRef.current) {
              cancelledRef.current = false;
              return;
            }
            const next = commitFromDom(event.target.value);
            onBlur?.(next);
            requestAnimationFrame(() => resizeTitle());
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
              const ed = editorRef.current;
              if (!ed) {
                return;
              }
              requestAnimationFrame(() => {
                const editor = editorRef.current;
                if (!editor || editor.isDestroyed) return;
                editor.chain().focus("start").scrollIntoView().run();
              });
            }
            if (event.key === "Escape") {
              event.preventDefault();
              cancelledRef.current = true;
              onCancel?.();
              event.currentTarget.blur();
            }
          }}
          onPaste={(event) => {
            event.preventDefault();
            const el = event.currentTarget;
            const pasted = event.clipboardData.getData("text/plain");
            const start = el.selectionStart ?? el.value.length;
            const end = el.selectionEnd ?? start;
            const merged = normalizeTitle(
              `${el.value.slice(0, start)}${pasted}${el.value.slice(end)}`,
            );
            el.value = merged;
            const caret = Math.min(start + pasted.length, merged.length);
            try {
              el.setSelectionRange(caret, caret);
            } catch {
              // Ignore selection errors on detached nodes.
            }
            rememberSelection();
            setLiveLen(merged.length);
            if (merged !== value) {
              onChange(merged);
            }
            requestAnimationFrame(() => resizeTitle());
          }}
        />
        {showCount ? (
          <span
            className={cn(
              "iris-doc-title-count",
              liveLen > NOTE_TITLE_HARD_LIMIT && "is-warning",
            )}
            aria-hidden
            title={
              liveLen > NOTE_TITLE_HARD_LIMIT
                ? "标题已达上限"
                : "标题较长可能影响 tab 显示"
            }
          >
            {liveLen}/{NOTE_TITLE_HARD_LIMIT}
          </span>
        ) : null}
      </div>
    </DocumentTitleContextMenu>
  );
}

export const DocumentTitleField = memo(DocumentTitleFieldInner);
DocumentTitleField.displayName = "DocumentTitleField";
