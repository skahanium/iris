import { Paperclip, Send, Square, X } from "lucide-react";
import type {
  ClipboardEvent,
  DragEvent,
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
import type { DisplayMention, SecurityDomain } from "@/types/ai";
import type { ImageAttachmentDto } from "@/types/ipc";
import { cn } from "@/lib/utils";

export interface AssistantComposerHandle {
  appendPlainText: (text: string) => void;
  clear: () => void;
  focus: () => void;
  getEditor: () => Editor | null;
  insertMention: (candidate: MentionCandidate) => boolean;
}

interface AiComposerProps {
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
  domain?: SecurityDomain;
  mentionEnabled?: boolean;
  getMentionCandidates?: (
    prefix: "@" | "#",
    query: string,
  ) => MentionCandidate[];
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
  contextShelf?: ReactNode;
  images?: ImageAttachmentDto[];
  onImagesChange?: (images: ImageAttachmentDto[]) => void;
}

const MAX_IMAGE_SIZE = 20 * 1024 * 1024;
const ALLOWED_MIME = ["image/png", "image/jpeg", "image/webp", "image/gif"];

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      resolve((reader.result as string).split(",")[1] ?? "");
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

async function processImageFiles(files: File[]): Promise<ImageAttachmentDto[]> {
  const output: ImageAttachmentDto[] = [];
  for (const file of files) {
    if (file.size > MAX_IMAGE_SIZE || !ALLOWED_MIME.includes(file.type))
      continue;
    output.push({
      id: crypto.randomUUID(),
      dataBase64: await fileToBase64(file),
      mimeType: file.type,
      fileName: file.name,
      sizeBytes: file.size,
    });
  }
  return output;
}

interface ComposerSnapshot {
  doc: JSONContent;
  text: string;
  displayMentions: DisplayMention[];
}

/** AI sidecar Composer with a plain-text projection and atomic local mentions. */
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
  domain = "normal",
  mentionEnabled = domain === "normal",
  getMentionCandidates = () => [],
  onKeyDown,
  contextShelf,
  images,
  onImagesChange,
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
  const imagesRef = useRef(images);
  imagesRef.current = images;
  const domainRef = useRef(domain);
  domainRef.current = domain;
  const activeDomainRef = useRef(domain);
  const emittedTextRef = useRef(value);
  const snapshotsRef = useRef<
    Partial<Record<SecurityDomain, ComposerSnapshot>>
  >({});
  const fileInputRef = useRef<HTMLInputElement>(null);

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
              (valueRef.current.trim() || imagesRef.current?.length)
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
    snapshotsRef.current[domainRef.current] = snapshot;
    emittedTextRef.current = projection.text;
    onChangeRef.current(projection.text, projection.displayMentions);
  }, []);
  emitProjectionRef.current = emitProjection;

  useEffect(() => {
    if (!editor) return;
    const previousDomain = activeDomainRef.current;
    if (previousDomain !== domain) {
      activeDomainRef.current = domain;
      const snapshot = snapshotsRef.current[domain];
      editor.commands.setContent(
        snapshot?.doc ?? assistantComposerDocFromText(value),
        false,
      );
      emitProjection(editor);
      return;
    }
    if (value === emittedTextRef.current) return;
    editor.commands.setContent(assistantComposerDocFromText(value), false);
    snapshotsRef.current[domain] = {
      doc: editor.getJSON(),
      text: value,
      displayMentions: [],
    };
    emittedTextRef.current = value;
    onChangeRef.current(value, []);
  }, [domain, editor, emitProjection, value]);

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

  const handlePaste = useCallback(
    async (event: ClipboardEvent<HTMLDivElement>) => {
      if (!onImagesChange) return;
      const files = Array.from(event.clipboardData.items)
        .filter((item) => item.type.startsWith("image/"))
        .map((item) => item.getAsFile())
        .filter((file): file is File => file !== null);
      if (files.length === 0) return;
      event.preventDefault();
      const next = await processImageFiles(files);
      if (next.length > 0) onImagesChange([...(images ?? []), ...next]);
    },
    [images, onImagesChange],
  );

  const handleDrop = useCallback(
    async (event: DragEvent<HTMLDivElement>) => {
      if (!onImagesChange) return;
      const files = Array.from(event.dataTransfer.files).filter((file) =>
        file.type.startsWith("image/"),
      );
      if (files.length === 0) return;
      event.preventDefault();
      const next = await processImageFiles(files);
      if (next.length > 0) onImagesChange([...(images ?? []), ...next]);
    },
    [images, onImagesChange],
  );

  const handleFileSelect = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      if (!onImagesChange) return;
      const next = await processImageFiles(
        Array.from(event.target.files ?? []),
      );
      if (next.length > 0) onImagesChange([...(images ?? []), ...next]);
      event.target.value = "";
    },
    [images, onImagesChange],
  );

  return (
    <div
      className={cn(
        "shrink-0 border-t border-border-subtle bg-ai-composer p-3",
        className,
      )}
      onDrop={(event) => void handleDrop(event)}
      onDragOver={(event) => {
        if (Array.from(event.dataTransfer.types).includes("Files"))
          event.preventDefault();
      }}
    >
      <div className="ai-composer-workbench relative rounded-lg border border-border/80 bg-surface-elevated focus-within:ring-2 focus-within:ring-primary/25">
        {contextShelf}
        <div className="flex items-end gap-2 p-2">
          <div className="flex min-w-0 flex-1 flex-col">
            {images && images.length > 0 ? (
              <div className="mb-1.5 flex flex-wrap gap-1.5">
                {images.map((image) => (
                  <div
                    key={image.id}
                    className="group relative h-10 w-10 overflow-hidden rounded-md border border-border/50"
                  >
                    <img
                      src={`data:${image.mimeType};base64,${image.dataBase64}`}
                      className="h-full w-full object-cover"
                      alt={image.fileName || ""}
                    />
                    <button
                      type="button"
                      className="absolute -right-0.5 -top-0.5 flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-destructive-foreground opacity-0 transition-opacity group-hover:opacity-100"
                      onClick={() =>
                        onImagesChange?.(
                          (images ?? []).filter((item) => item.id !== image.id),
                        )
                      }
                      aria-label="移除图片"
                    >
                      <X className="h-2.5 w-2.5" />
                    </button>
                  </div>
                ))}
              </div>
            ) : null}
            <div
              className={cn(
                "ai-composer-editor-shell max-h-32 min-h-[2.5rem] overflow-y-auto",
                editor?.isEmpty && "is-empty",
              )}
              onPaste={(event) => void handlePaste(event)}
            >
              <EditorContent editor={editor} />
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {onImagesChange ? (
              <>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept="image/*"
                  multiple
                  className="hidden"
                  onChange={(event) => void handleFileSelect(event)}
                />
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="h-8 w-8"
                  onClick={() => fileInputRef.current?.click()}
                  aria-label="添加图片"
                >
                  <Paperclip className="h-4 w-4" />
                </Button>
              </>
            ) : null}
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
                  (!value.trim() && !(images && images.length > 0))
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
