import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ImageExtension } from "@/components/editor/extensions/ImageExtension";
import { WikiMediaEmbedExtension } from "@/components/editor/extensions/WikiMediaEmbedExtension";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

const mediaResolve = vi.fn();
const mediaRelease = vi.fn();

vi.mock("@/lib/ipc", () => ({
  mediaResolve: (...args: unknown[]) => mediaResolve(...args),
  mediaRelease: (...args: unknown[]) => mediaRelease(...args),
}));

function createEditorWithImageMediaMode(mediaLoading: "deferred" | "visible") {
  const { tipTapHtml } = ingestMarkdownForEditor({
    bodyMarkdown: "![diagram](assets/example.png)",
  });
  return new Editor({
    extensions: [
      StarterKit,
      ImageExtension.configure({
        mediaLoading,
        vaultPath: "/Users/example/Vault",
      } as Parameters<typeof ImageExtension.configure>[0] & {
        mediaLoading: "deferred" | "visible";
      }),
      WikiMediaEmbedExtension.configure({ mediaLoading, vaultPath: null }),
    ],
    content: tipTapHtml,
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

describe("editor media deferred loading", () => {
  beforeEach(() => {
    mediaResolve.mockReset();
    mediaRelease.mockReset();
    mediaResolve.mockResolvedValue({
      handle: "lease-1",
      mediaKind: "image",
      mimeType: "image/png",
      path: "diagram.png",
      sizeBytes: 1,
      updatedAt: null,
      url: "iris-media://localhost/lease-1",
    });
  });

  it("does not attach real image src while a surface is hidden or staging", () => {
    const editor = createEditorWithImageMediaMode("deferred");
    try {
      const image = editor.view.dom.querySelector("img");
      expect(image).toBeTruthy();
      expect(image?.getAttribute("data-iris-media-src")).toBe(
        "assets/example.png",
      );
      expect(image?.hasAttribute("src")).toBe(false);
    } finally {
      editor.destroy();
    }
  });

  it("uses lazy async browser loading when media is visible", () => {
    const editor = createEditorWithImageMediaMode("visible");
    try {
      const image = editor.view.dom.querySelector("img");
      expect(image).toBeTruthy();
      expect(image?.getAttribute("src")).toBe("assets/example.png");
      expect(image?.getAttribute("loading")).toBe("lazy");
      expect(image?.getAttribute("decoding")).toBe("async");
    } finally {
      editor.destroy();
    }
  });
});

describe("wiki media embed deferred loading", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    mediaResolve.mockReset();
    mediaRelease.mockReset();
  });

  it("renders pdf embeds through an opaque media lease", async () => {
    mediaResolve.mockResolvedValue({
      handle: "lease-pdf",
      mediaKind: "pdf",
      mimeType: "application/pdf",
      path: "paper.pdf",
      sizeBytes: 10,
      updatedAt: null,
      url: "iris-media://localhost/lease-pdf",
    });
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "![[paper.pdf|证据材料]]",
    });
    const editor = new Editor({
      extensions: [
        StarterKit,
        WikiMediaEmbedExtension.configure({
          mediaLoading: "visible",
          vaultPath: null,
        }),
      ],
      content: tipTapHtml,
      parseOptions: EDITOR_PARSE_OPTIONS,
    });
    try {
      const embed = editor.view.dom.querySelector("[data-wiki-media-embed]");
      expect(embed).toBeTruthy();
      expect(embed?.getAttribute("data-media-kind")).toBe("pdf");
      expect(embed?.textContent).toContain("证据材料");
      expect(editor.view.dom.querySelector("img")).toBeNull();
      await vi.waitFor(() => {
        expect(
          editor.view.dom.querySelector("object")?.getAttribute("data"),
        ).toBe("iris-media://localhost/lease-pdf");
      });
    } finally {
      editor.destroy();
      expect(mediaRelease).toHaveBeenCalledWith("lease-pdf");
    }
  });

  it("defers image media embeds until their surface is visible", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "![[diagram.png|示意图]]",
    });
    const editor = new Editor({
      extensions: [
        StarterKit,
        WikiMediaEmbedExtension.configure({
          mediaLoading: "deferred",
          vaultPath: null,
        }),
      ],
      content: tipTapHtml,
      parseOptions: EDITOR_PARSE_OPTIONS,
    });
    try {
      const image = editor.view.dom.querySelector("img");
      expect(image).toBeTruthy();
      expect(image?.getAttribute("data-iris-media-src")).toBe("diagram.png");
      expect(image?.hasAttribute("src")).toBe(false);
      expect(image?.getAttribute("alt")).toBe("示意图");
      expect(mediaResolve).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("resolves visible wiki media only after it approaches the viewport", async () => {
    const observers: Array<{
      callback: IntersectionObserverCallback;
      disconnect: ReturnType<typeof vi.fn>;
    }> = [];
    vi.stubGlobal(
      "IntersectionObserver",
      class {
        readonly callback: IntersectionObserverCallback;
        readonly disconnect = vi.fn();

        constructor(callback: IntersectionObserverCallback) {
          this.callback = callback;
          observers.push(this);
        }

        observe() {}
        takeRecords() {
          return [];
        }
        unobserve() {}
      },
    );
    mediaResolve.mockResolvedValue({
      handle: "lease-image",
      mediaKind: "image",
      mimeType: "image/png",
      path: "diagram.png",
      sizeBytes: 10,
      updatedAt: null,
      url: "iris-media://localhost/lease-image",
    });
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "![[diagram.png|示意图]]",
    });
    const editor = new Editor({
      extensions: [
        StarterKit,
        WikiMediaEmbedExtension.configure({
          mediaLoading: "visible",
          vaultPath: null,
        }),
      ],
      content: tipTapHtml,
      parseOptions: EDITOR_PARSE_OPTIONS,
    });
    try {
      expect(mediaResolve).not.toHaveBeenCalled();
      expect(observers).toHaveLength(1);
      observers[0]!.callback(
        [{ isIntersecting: true } as IntersectionObserverEntry],
        {} as IntersectionObserver,
      );
      await vi.waitFor(() => {
        expect(mediaResolve).toHaveBeenCalledWith("diagram.png");
      });
    } finally {
      editor.destroy();
      vi.unstubAllGlobals();
    }
  });
});
