import type { Editor } from "@tiptap/react";
import { useRef, type RefObject } from "react";

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
  onBlur?: () => void;
  editorRef: RefObject<Editor | null>;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
}

export function DocumentTitleField({
  value,
  onChange,
  onBlur,
  editorRef,
  disabled = false,
  placeholder = "未命名文档",
  className,
}: DocumentTitleFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const len = value.length;
  const showCount = len > NOTE_TITLE_SOFT_LIMIT;

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
    >
      <div
        className={cn("iris-document-title-field iris-doc-title-wrap", className)}
        data-testid="document-title-field"
      >
        <input
          ref={inputRef}
          type="text"
          data-testid="document-title"
          className="iris-doc-title"
          value={value}
          disabled={disabled}
          placeholder={placeholder}
          aria-label="文档标题"
          onChange={(event) => commit(event.target.value)}
          onBlur={onBlur}
          onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            const ed = editorRef.current;
            if (!ed) {
              (event.target as HTMLInputElement).blur();
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
