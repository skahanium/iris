import { Send, Square } from "lucide-react";
import type {
  ClipboardEvent,
  KeyboardEvent,
  ReactNode,
  RefObject,
} from "react";
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
} from "react";
import { EditorContent, useEditor } from "@tiptap/react";
import type { Editor, JSONContent } from "@tiptap/core";

import { Button } from "@/components/ui/button";
import type { MentionCandidate } from "@/lib/ai-context-scope";
import {
  assistantComposerDocFromText,
  projectAssistantComposerDoc,
} from "@/lib/assistant-composer-doc";
import {
  AssistantMentionExtension,
  createAssistantComposerExtensions,
  insertAssistantMention,
} from "@/lib/assistant-composer-extensions";
import type { DisplayMention } from "@/types/ai";
import { cn } from "@/lib/utils";

export interface AssistantComposerHandle {
  appendPlainText: (text: string) => void;
  clear: () => void;
  focus: () => void;
  getEditor: () => Editor | null;
  insertMention: (candidate: MentionCandidate) => boolean;
}

export interface AiComposerProps {
  value: string;
  displayMentions?: DisplayMention[];
  onChange: (value: string, mentions: DisplayMention[]) => void;
  onSubmit: () => void;
  onStop?: () => void;
  streaming?: boolean;
  disabled?: boolean;
  submitDisabled?: boolean;
  placeholder?: string;
  className?: string;
  composerRef?: RefObject<AssistantComposerHandle | null>;
  scopeKey?: string;
  mentionEnabled?: boolean;
  getMentionCandidates?: (
    prefix: "@" | "#",
    query: string,
  ) => MentionCandidate[];
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
  header?: ReactNode;
  leadingActions?: ReactNode;
  hasSupplementalContent?: boolean;
  onPaste?: (event: ClipboardEvent<HTMLDivElement>) => void;
}

interface ComposerSnapshot {
  doc: JSONContent;
  text: string;
  displayMentions: DisplayMention[];
}

/** Generic TipTap Composer primitive with a plain-text projection and atomic mentions. */
export function AiComposer({
  value,
  onChange,
  onSubmit,
  onStop,
  streaming = false,
  disabled = false,
  submitDisabled = false,
  placeholder = "提问…",
  className,
  composerRef,
  scopeKey = "default",
  mentionEnabled = true,
  getMentionCandidates = () => [],
  onKeyDown,
  header,
  leadingActions,
  hasSupplementalContent = false,
  onPaste,
}: AiComposerProps) {
  const getCandidatesRef = useRef(getMentionCandidates);
  getCandidatesRef.current = getMentionCandidates;
  const mentionEnabledRef = useRef(mentionEnabled);
  mentionEnabledRef.current = mentionEnabled;
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSubmitRef = useRef(onSubmit);
  onSubmitRef.current = onSubmit;
  const onKeyDownRef = useRef(onKeyDown);
  onKeyDownRef.current = onKeyDown;
  const valueRef = useRef(value);
  valueRef.current = value;
  const streamingRef = useRef(streaming);
  streamingRef.current = streaming;
  const submitDisabledRef = useRef(submitDisabled);
  submitDisabledRef.current = submitDisabled;
  const supplementalContentRef = useRef(hasSupplementalContent);
  supplementalContentRef.current = hasSupplementalContent;
  const scopeKeyRef = useRef(scopeKey);
  scopeKeyRef.current = scopeKey;
  const activeScopeKeyRef = useRef(scopeKey);
  const emittedTextRef = useRef(value);
  const snapshotsRef = useRef<Record<string, ComposerSnapshot>>({});

  const mentionExtension = useMemo(
    () =>
      AssistantMentionExtension.configure({
        enabled: () => mentionEnabledRef.current,
        getCandidates: (prefix, query) =>
          getCandidatesRef.current(prefix, query),
      }),
    [],
  );
  const extensions = useMemo(
    () => createAssistantComposerExtensions({ mentionExtension }),
    [mentionExtension],
  );
  const emitProjectionRef = useRef<(instance: Editor) => void>(() => undefined);

  const editor = useEditor(
    {
      extensions,
      content: assistantComposerDocFromText(value),
      immediatelyRender: true,
      shouldRerenderOnTransaction: false,
      editorProps: {
        attributes: {
          "aria-label": "AI 输入",
          class: "ai-composer-editor prose-none outline-none",
          "data-placeholder": placeholder,
          role: "textbox",
        },
        handleKeyDown: (_view, event) => {
          if (event.isComposing || event.keyCode === 229) return false;
          onKeyDownRef.current?.(
            event as unknown as KeyboardEvent<HTMLDivElement>,
          );
          if (event.defaultPrevented) return true;
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            if (
              !streamingRef.current &&
              !submitDisabledRef.current &&
              (valueRef.current.trim() || supplementalContentRef.current)
            ) {
              onSubmitRef.current();
            }
            return true;
          }
          return false;
        },
      },
      onUpdate: ({ editor: updatedEditor }) => {
        emitProjectionRef.current(updatedEditor);
      },
    },
    [],
  );

  const emitProjection = useCallback((instance: Editor) => {
    const projection = projectAssistantComposerDoc(instance.state.doc);
    const snapshot: ComposerSnapshot = {
      doc: instance.getJSON(),
      text: projection.text,
      displayMentions: projection.displayMentions,
    };
    snapshotsRef.current[scopeKeyRef.current] = snapshot;
    emittedTextRef.current = projection.text;
    onChangeRef.current(projection.text, projection.displayMentions);
  }, []);
  emitProjectionRef.current = emitProjection;

  useEffect(() => {
    if (!editor) return;
    const previousScopeKey = activeScopeKeyRef.current;
    if (previousScopeKey !== scopeKey) {
      activeScopeKeyRef.current = scopeKey;
      const snapshot = snapshotsRef.current[scopeKey];
      editor.commands.setContent(
        snapshot?.doc ?? assistantComposerDocFromText(value),
        false,
      );
      emitProjection(editor);
      return;
    }
    if (value === emittedTextRef.current) return;
    editor.commands.setContent(assistantComposerDocFromText(value), false);
    snapshotsRef.current[scopeKey] = {
      doc: editor.getJSON(),
      text: value,
      displayMentions: [],
    };
    emittedTextRef.current = value;
    onChangeRef.current(value, []);
  }, [editor, emitProjection, scopeKey, value]);

  useImperativeHandle(
    composerRef,
    () => ({
      appendPlainText(text) {
        if (!editor || !text) return;
        editor.chain().focus().insertContent(text).run();
      },
      clear() {
        if (!editor) return;
        editor.commands.clearContent(true);
        emitProjection(editor);
      },
      focus() {
        editor?.commands.focus();
      },
      getEditor: () => editor,
      insertMention: (candidate) =>
        editor ? insertAssistantMention(editor, candidate) : false,
    }),
    [editor, emitProjection],
  );

  return (
    <div
      className={cn(
        "shrink-0 border-t border-border-subtle bg-ai-composer p-3",
        className,
      )}
    >
      <div className="ai-composer-workbench relative rounded-lg border border-border/80 bg-surface-elevated focus-within:ring-2 focus-within:ring-primary/25">
        {header}
        <div className="flex items-end gap-2 p-2">
          <div className="flex min-w-0 flex-1 flex-col">
            <div
              className={cn(
                "ai-composer-editor-shell max-h-32 min-h-[2.5rem] overflow-y-auto",
                editor?.isEmpty && "is-empty",
              )}
              onPaste={onPaste}
            >
              <EditorContent editor={editor} />
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {leadingActions}
            {streaming && onStop ? (
              <Button
                type="button"
                size="icon"
                variant="secondary"
                className="h-9 w-9"
                aria-label="停止生成"
                onClick={onStop}
              >
                <Square className="h-3.5 w-3.5" />
              </Button>
            ) : (
              <Button
                type="button"
                size="icon"
                variant="brand"
                className="h-9 w-9"
                disabled={
                  disabled ||
                  submitDisabled ||
                  (!value.trim() && !hasSupplementalContent)
                }
                aria-label="发送"
                onClick={onSubmit}
              >
                <Send className="h-4 w-4" />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
