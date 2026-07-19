import type { Editor } from "@tiptap/react";
import {
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
 * Parent state and path-sync commit on blur (and context-menu edits).
 * When blurred, external `value` updates are mirrored into the DOM.
 */
export function DocumentTitleField({
  value,
  resetKey,
  onChange,
  onBlur,
  editorRef,
  disabled = false,
  readOnly = false,
  placeholder = "未命名文档",
  className,
}: DocumentTitleFieldProps) {
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const focusedRef = useRef(false);
  const [focused, setFocused] = useState(false);
  const [liveLen, setLiveLen] = useState(value.length);
  const len = focused ? liveLen : value.length;
  const showCount = len > NOTE_TITLE_SOFT_LIMIT;

  const resizeTitle = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
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
  }, [focused, resizeTitle, value, resetKey]);

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
          key={resetKey}
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
            const next = normalizeTitle(event.currentTarget.value);
            setLiveLen(next.length);
            resizeTitle();
            if (next !== value) {
              onChange(next);
            }
          }}
          onFocus={() => {
            focusedRef.current = true;
            setFocused(true);
            requestAnimationFrame(resizeTitle);
          }}
          onBlur={(event) => {
            focusedRef.current = false;
            setFocused(false);
            const next = commitFromDom(event.target.value);
            onBlur?.(next);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              const ed = editorRef.current;
              if (!ed) {
                (event.target as HTMLTextAreaElement).blur();
                return;
              }
              requestAnimationFrame(() => {
                const editor = editorRef.current;
                if (!editor || editor.isDestroyed) return;
                editor.chain().focus("start").scrollIntoView().run();
              });
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
            setLiveLen(merged.length);
            if (merged !== value) {
              onChange(merged);
            }
            requestAnimationFrame(resizeTitle);
          }}
        />
        {showCount ? (
          <span
            className={cn(
              "iris-doc-title-count",
              len > NOTE_TITLE_HARD_LIMIT && "is-warning",
            )}
            aria-hidden
            title={
              len > NOTE_TITLE_HARD_LIMIT
                ? "标题已达上限"
                : "标题较长可能影响 tab 显示"
            }
          >
            {len}/{NOTE_TITLE_HARD_LIMIT}
          </span>
        ) : null}
      </div>
    </DocumentTitleContextMenu>
  );
}
