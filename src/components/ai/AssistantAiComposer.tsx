import { Paperclip, X } from "lucide-react";
import {
  useCallback,
  useRef,
  type ClipboardEvent,
  type DragEvent,
  type ReactNode,
} from "react";

import { AiComposer, type AiComposerProps } from "@/components/ui/ai-composer";
import { Button } from "@/components/ui/button";
import type { SecurityDomain } from "@/types/ai";
import type { ImageAttachmentDto } from "@/types/ipc";

const MAX_IMAGE_SIZE = 20 * 1024 * 1024;
const ALLOWED_MIME = ["image/png", "image/jpeg", "image/webp", "image/gif"];

interface AssistantAiComposerProps extends Omit<
  AiComposerProps,
  | "scopeKey"
  | "mentionEnabled"
  | "header"
  | "leadingActions"
  | "hasSupplementalContent"
  | "onPaste"
> {
  domain: SecurityDomain;
  contextShelf?: ReactNode;
  images: ImageAttachmentDto[];
  onImagesChange: (images: ImageAttachmentDto[]) => void;
}

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

/** Business wrapper that owns image and security-domain policy around the TipTap primitive. */
export function AssistantAiComposer({
  domain,
  contextShelf,
  images,
  onImagesChange,
  ...composerProps
}: AssistantAiComposerProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const appendFiles = useCallback(
    async (files: File[]) => {
      const next = await processImageFiles(files);
      if (next.length > 0) onImagesChange([...images, ...next]);
    },
    [images, onImagesChange],
  );
  const handlePaste = useCallback(
    (event: ClipboardEvent<HTMLDivElement>) => {
      const files = Array.from(event.clipboardData.items)
        .filter((item) => item.type.startsWith("image/"))
        .map((item) => item.getAsFile())
        .filter((file): file is File => file !== null);
      if (files.length === 0) return;
      event.preventDefault();
      void appendFiles(files);
    },
    [appendFiles],
  );
  const handleDrop = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      const files = Array.from(event.dataTransfer.files).filter((file) =>
        file.type.startsWith("image/"),
      );
      if (files.length === 0) return;
      event.preventDefault();
      void appendFiles(files);
    },
    [appendFiles],
  );
  const imageStrip =
    images.length > 0 ? (
      <div className="mx-2 mt-2 flex flex-wrap gap-1.5">
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
                onImagesChange(images.filter((item) => item.id !== image.id))
              }
              aria-label="移除图片"
            >
              <X className="h-2.5 w-2.5" />
            </button>
          </div>
        ))}
      </div>
    ) : null;

  return (
    <div
      onDrop={handleDrop}
      onDragOver={(event) => {
        if (Array.from(event.dataTransfer.types).includes("Files"))
          event.preventDefault();
      }}
    >
      <AiComposer
        {...composerProps}
        scopeKey={domain}
        mentionEnabled={domain === "normal"}
        header={
          <>
            {contextShelf}
            {imageStrip}
          </>
        }
        hasSupplementalContent={images.length > 0}
        onPaste={handlePaste}
        leadingActions={
          <>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/*"
              multiple
              className="hidden"
              onChange={(event) => {
                void appendFiles(Array.from(event.target.files ?? []));
                event.target.value = "";
              }}
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
        }
      />
    </div>
  );
}
