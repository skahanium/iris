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
  onChange: (value: string) => void;
  onBlur?: (committedTitle: string) => void;
  editorRef: RefObject<Editor | null>;
  disabled?: boolean;
  readOnly?: boolean;
  placeholder?: string;
  className?: string;
}

export function DocumentTitleField({
  value,
  onChange,
  onBlur,
  editorRef,
  disabled = false,
  readOnly = false,
  placeholder = "未命名文档",
  className,
}: DocumentTitleFieldProps) {
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [focused, setFocused] = useState(false);
  const len = value.length;
  const showCount = len > NOTE_TITLE_SOFT_LIMIT;

  const resizeTitle = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, []);

  useLayoutEffect(() => {
    resizeTitle();
  }, [focused, resizeTitle, value]);

  const commit = (raw: string) => {
    const next = sanitizeDocumentTitleInput(raw).slice(
      0,
      NOTE_TITLE_HARD_LIMIT,
    );
    if (next !== value) {
      onChange(next);
    }
  };

  return (
    <DocumentTitleContextMenu
      inputRef={inputRef}
      value={value}
      onValueChange={commit}
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
          value={value}
          disabled={disabled}
          readOnly={readOnly}
          placeholder={placeholder}
          aria-label="文档标题"
          title={value || undefined}
          onChange={(event) => {
            commit(event.target.value);
            resizeTitle();
          }}
          onFocus={() => {
            setFocused(true);
            requestAnimationFrame(resizeTitle);
          }}
          onBlur={(event) => {
            setFocused(false);
            const next = sanitizeDocumentTitleInput(event.target.value).slice(
              0,
              NOTE_TITLE_HARD_LIMIT,
            );
            if (next !== value) {
              onChange(next);
            }
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
            const pasted = event.clipboardData.getData("text/plain");
            const merged = sanitizeDocumentTitleInput(value + pasted);
            commit(merged);
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
