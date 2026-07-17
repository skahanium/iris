import { Paperclip, Send, Square, X } from "lucide-react";
import type {
  CompositionEvent,
  KeyboardEvent,
  ReactNode,
  RefObject,
} from "react";
import { useCallback, useRef } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  displayMentionTooltip,
  validDisplayMentions,
  type MentionTextEdit,
} from "@/lib/ai-context-scope";
import type { DisplayMention } from "@/types/ai";
import type { ImageAttachmentDto } from "@/types/ipc";

interface AiComposerProps {
  value: string;
  onChange: (value: string, edit?: MentionTextEdit) => void;
  onSubmit: () => void;
  onStop?: () => void;
  streaming?: boolean;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
  textareaRef?: RefObject<HTMLTextAreaElement | null>;
  onComposerKeyDown?: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onCompositionStart?: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onCompositionEnd?: (e: CompositionEvent<HTMLTextAreaElement>) => void;
  onSelect?: () => void;
  mentionPopover?: ReactNode;
  displayMentions?: DisplayMention[];
  /** @deprecated 工具/检索状态已移至底栏，保留以兼容旧调用 */
  statusHint?: string | null;
  /** 已附加的图片列表 */
  images?: ImageAttachmentDto[];
  /** 图片列表变更回调 */
  onImagesChange?: (images: ImageAttachmentDto[]) => void;
}

const MAX_IMAGE_SIZE = 20 * 1024 * 1024; // 20MB
const ALLOWED_MIME = ["image/png", "image/jpeg", "image/webp", "image/gif"];

interface BeforeInputSnapshot {
  value: string;
  selectionStart: number;
  selectionEnd: number;
  inputType: string;
}

function editFromBeforeInput(
  snapshot: BeforeInputSnapshot,
  nextValue: string,
): MentionTextEdit | undefined {
  const { selectionStart, selectionEnd } = snapshot;
  if (selectionStart !== selectionEnd) {
    const insertedTextLength =
      nextValue.length -
      (snapshot.value.length - (selectionEnd - selectionStart));
    return insertedTextLength >= 0
      ? { from: selectionStart, to: selectionEnd, insertedTextLength }
      : undefined;
  }

  if (snapshot.inputType.startsWith("delete")) {
    const deletedTextLength = snapshot.value.length - nextValue.length;
    if (deletedTextLength < 0) return undefined;
    if (snapshot.inputType.endsWith("Backward")) {
      return {
        from: Math.max(0, selectionStart - deletedTextLength),
        to: selectionStart,
        insertedTextLength: 0,
      };
    }
    return {
      from: selectionStart,
      to: Math.min(snapshot.value.length, selectionStart + deletedTextLength),
      insertedTextLength: 0,
    };
  }

  if (snapshot.inputType.startsWith("insert")) {
    const insertedTextLength = nextValue.length - snapshot.value.length;
    return insertedTextLength >= 0
      ? {
          from: selectionStart,
          to: selectionStart,
          insertedTextLength,
        }
      : undefined;
  }

  return undefined;
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = (reader.result as string).split(",")[1] ?? "";
      resolve(result);
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

async function processImageFiles(files: File[]): Promise<ImageAttachmentDto[]> {
  const out: ImageAttachmentDto[] = [];
  for (const file of files) {
    if (file.size > MAX_IMAGE_SIZE) continue;
    if (!ALLOWED_MIME.includes(file.type)) continue;
    const dataBase64 = await fileToBase64(file);
    out.push({
      id: crypto.randomUUID(),
      dataBase64,
      mimeType: file.type,
      fileName: file.name,
      sizeBytes: file.size,
    });
  }
  return out;
}

/** AI 侧栏多行输入区 */
export function AiComposer({
  value,
  onChange,
  onSubmit,
  onStop,
  streaming = false,
  disabled = false,
  placeholder = "提问…",
  className,
  textareaRef,
  onComposerKeyDown,
  onCompositionStart,
  onCompositionEnd,
  onSelect,
  mentionPopover,
  displayMentions = [],
  images,
  onImagesChange,
}: AiComposerProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const mentionLayerRef = useRef<HTMLDivElement>(null);
  const composingRef = useRef(false);
  const beforeInputRef = useRef<BeforeInputSnapshot | null>(null);
  const selectionSnapshotRef = useRef<BeforeInputSnapshot | null>(null);
  const visibleMentions = validDisplayMentions(value, displayMentions);

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    selectionSnapshotRef.current = {
      value: e.currentTarget.value,
      selectionStart: e.currentTarget.selectionStart,
      selectionEnd: e.currentTarget.selectionEnd,
      inputType: "",
    };
    if (
      composingRef.current ||
      e.nativeEvent.isComposing ||
      e.nativeEvent.keyCode === 229
    ) {
      return;
    }
    onComposerKeyDown?.(e);
    if (e.defaultPrevented) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!streaming && (value.trim() || (images && images.length > 0)))
        onSubmit();
    }
  };

  const handleBeforeInput = (event: React.FormEvent<HTMLTextAreaElement>) => {
    const nativeEvent = event.nativeEvent as InputEvent;
    beforeInputRef.current = {
      value: event.currentTarget.value,
      selectionStart: event.currentTarget.selectionStart,
      selectionEnd: event.currentTarget.selectionEnd,
      inputType:
        nativeEvent.inputType ||
        (typeof nativeEvent.data === "string" ? "insertText" : ""),
    };
  };

  const captureSelection = (textarea: HTMLTextAreaElement) => {
    selectionSnapshotRef.current = {
      value: textarea.value,
      selectionStart: textarea.selectionStart,
      selectionEnd: textarea.selectionEnd,
      inputType: "",
    };
  };

  const handleSelection = (
    event: React.SyntheticEvent<HTMLTextAreaElement>,
  ) => {
    captureSelection(event.currentTarget);
    onSelect?.();
  };

  const handleCompositionStart = (
    event: CompositionEvent<HTMLTextAreaElement>,
  ) => {
    composingRef.current = true;
    onCompositionStart?.(event);
  };

  const handleCompositionEnd = (
    event: CompositionEvent<HTMLTextAreaElement>,
  ) => {
    composingRef.current = false;
    onCompositionEnd?.(event);
  };

  const handlePaste = useCallback(
    async (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
      if (!onImagesChange) return;
      const items = Array.from(e.clipboardData.items);
      const imageFiles = items
        .filter((item) => item.type.startsWith("image/"))
        .map((item) => item.getAsFile())
        .filter((f): f is File => f !== null);
      if (imageFiles.length > 0) {
        e.preventDefault();
        const newImages = await processImageFiles(imageFiles);
        if (newImages.length > 0) {
          onImagesChange([...(images ?? []), ...newImages]);
        }
      }
    },
    [images, onImagesChange],
  );

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      if (!onImagesChange) return;
      const files = Array.from(e.dataTransfer.files).filter((f) =>
        f.type.startsWith("image/"),
      );
      if (files.length > 0) {
        e.preventDefault();
        const newImages = await processImageFiles(files);
        if (newImages.length > 0) {
          onImagesChange([...(images ?? []), ...newImages]);
        }
      }
    },
    [images, onImagesChange],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    if (Array.from(e.dataTransfer.types).includes("Files")) {
      e.preventDefault();
    }
  }, []);

  const handleFileSelect = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      if (!onImagesChange) return;
      const files = Array.from(e.target.files || []);
      if (files.length > 0) {
        const newImages = await processImageFiles(files);
        if (newImages.length > 0) {
          onImagesChange([...(images ?? []), ...newImages]);
        }
      }
      e.target.value = "";
    },
    [images, onImagesChange],
  );

  const removeImage = useCallback(
    (id: string) => {
      onImagesChange?.((images ?? []).filter((i) => i.id !== id));
    },
    [images, onImagesChange],
  );

  const handleTextAreaScroll = useCallback(
    (event: React.UIEvent<HTMLTextAreaElement>) => {
      const layer = mentionLayerRef.current;
      if (!layer) return;
      layer.scrollTop = event.currentTarget.scrollTop;
      layer.scrollLeft = event.currentTarget.scrollLeft;
    },
    [],
  );

  const mentionOverlay = (() => {
    if (visibleMentions.length === 0) return null;
    const nodes: ReactNode[] = [];
    let cursor = 0;
    visibleMentions.forEach((mention, index) => {
      if (mention.range.from > cursor) {
        nodes.push(value.slice(cursor, mention.range.from));
      }
      nodes.push(
        <span
          key={`${mention.kind}:${mention.value}:${mention.range.from}:${index}`}
          className="ai-composer-display-mention pointer-events-auto cursor-help"
          title={displayMentionTooltip(mention)}
          onMouseDown={(event) => {
            event.preventDefault();
            event.stopPropagation();
            const textarea =
              event.currentTarget.parentElement?.parentElement?.querySelector<HTMLTextAreaElement>(
                "textarea",
              );
            if (!textarea) return;
            textarea.focus();
            textarea.setSelectionRange(mention.range.to, mention.range.to);
          }}
        >
          {value.slice(mention.range.from, mention.range.to)}
        </span>,
      );
      cursor = mention.range.to;
    });
    if (cursor < value.length) nodes.push(value.slice(cursor));
    return nodes;
  })();

  return (
    <div
      className={cn(
        "shrink-0 border-t border-border/60 bg-ai-composer p-3",
        className,
      )}
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <div className="ai-composer-workbench relative flex items-end gap-2 rounded-lg border border-border/80 bg-surface-inset/50 p-2 shadow-sm focus-within:ring-2 focus-within:ring-primary/25">
        {mentionPopover ? (
          <div className="absolute bottom-full left-0 right-0 z-20 mb-1.5">
            {mentionPopover}
          </div>
        ) : null}
        <div className="flex min-w-0 flex-1 flex-col">
          {images && images.length > 0 && (
            <div className="mb-1.5 flex flex-wrap gap-1.5">
              {images.map((img) => (
                <div
                  key={img.id}
                  className="group relative h-10 w-10 overflow-hidden rounded-md border border-border/50"
                >
                  <img
                    src={`data:${img.mimeType};base64,${img.dataBase64}`}
                    className="h-full w-full object-cover"
                    alt={img.fileName || ""}
                  />
                  <button
                    type="button"
                    className="absolute -right-0.5 -top-0.5 flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-destructive-foreground opacity-0 transition-opacity group-hover:opacity-100"
                    onClick={() => removeImage(img.id)}
                    aria-label="移除图片"
                  >
                    <X className="h-2.5 w-2.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
          <div className="relative min-h-[2.5rem] w-full">
            {mentionOverlay ? (
              <div
                ref={mentionLayerRef}
                aria-hidden="true"
                data-testid="ai-mention-highlight-layer"
                className="ai-composer-mention-layer pointer-events-none absolute inset-0 z-[2] max-h-32 min-h-[2.5rem] overflow-hidden whitespace-pre-wrap break-words text-[15px] leading-[1.52] text-foreground"
              >
                {mentionOverlay}
              </div>
            ) : null}
            <textarea
              ref={textareaRef}
              rows={2}
              value={value}
              disabled={disabled && !streaming}
              placeholder={
                images && images.length > 0 ? "描述图片内容…" : placeholder
              }
              aria-label="AI 输入"
              className={cn(
                "relative z-[1] max-h-32 min-h-[2.5rem] w-full resize-none bg-transparent text-[15px] leading-[1.52] text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50",
                mentionOverlay && "ai-composer-textarea-with-mentions",
              )}
              onBeforeInput={handleBeforeInput}
              onChange={(event) => {
                const nativeEvent = event.nativeEvent as InputEvent;
                const baseSnapshot =
                  beforeInputRef.current ?? selectionSnapshotRef.current;
                beforeInputRef.current = null;
                const snapshot = baseSnapshot
                  ? {
                      ...baseSnapshot,
                      inputType:
                        baseSnapshot.inputType ||
                        nativeEvent.inputType ||
                        (typeof nativeEvent.data === "string"
                          ? "insertText"
                          : ""),
                    }
                  : null;
                onChange(
                  event.target.value,
                  snapshot
                    ? editFromBeforeInput(snapshot, event.target.value)
                    : undefined,
                );
                captureSelection(event.currentTarget);
              }}
              onCompositionStart={handleCompositionStart}
              onCompositionEnd={handleCompositionEnd}
              onKeyDown={handleKeyDown}
              onPaste={handlePaste}
              onScroll={handleTextAreaScroll}
              onSelect={handleSelection}
              onClick={handleSelection}
            />
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {onImagesChange && (
            <>
              <input
                ref={fileInputRef}
                type="file"
                accept="image/*"
                multiple
                className="hidden"
                onChange={handleFileSelect}
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
          )}
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
              className="h-9 w-9"
              disabled={
                disabled || (!value.trim() && !(images && images.length > 0))
              }
              aria-label="发送"
              onClick={onSubmit}
            >
              <Send className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
      <p className="mt-1.5 text-[10px] text-muted-foreground">
        Enter 发送 · Shift+Enter 换行
        {onImagesChange && " · 粘贴/拖拽图片"}
      </p>
    </div>
  );
}
