import { mergeAttributes, Node, type NodeViewRenderer } from "@tiptap/core";
import { convertFileSrc } from "@tauri-apps/api/core";

import { isTauriRuntime } from "@/lib/tauri-runtime";

function isSafeImageSrc(src: string): boolean {
  const trimmed = src.trim();
  if (!trimmed) return false;
  if (/^(https?:|file:|\/|\.\/|\.\.\/)/i.test(trimmed)) return true;
  return !/^[a-z][a-z0-9+.-]*:/i.test(trimmed);
}

function isVaultAssetSrc(src: string): boolean {
  const normalized = src.trim().replace(/\\/g, "/");
  if (!normalized.startsWith("assets/")) return false;
  const name = normalized.slice("assets/".length);
  return Boolean(name) && !name.endsWith("/") && !name.includes("..");
}

function joinVaultAssetPath(vaultPath: string, src: string): string {
  const base = vaultPath.trim().replace(/[\\/]+$/, "");
  const separator = base.includes("\\") ? "\\" : "/";
  const relative = src
    .trim()
    .replace(/^[/\\]+/, "")
    .replace(/[\\/]+/g, separator);
  return `${base}${separator}${relative}`;
}

function renderImageSrc(src: string, vaultPath: string | null): string {
  if (!vaultPath || !isTauriRuntime() || !isVaultAssetSrc(src)) {
    return src;
  }
  return convertFileSrc(joinVaultAssetPath(vaultPath, src));
}

function setFrameState(
  frame: HTMLElement,
  state: "deferred" | "pending" | "loaded" | "error",
): void {
  frame.dataset.mediaState = state;
  if (state === "error") {
    frame.dataset.mediaError = "true";
  } else {
    delete frame.dataset.mediaError;
  }
}

interface ImageExtensionOptions {
  mediaLoading: "deferred" | "visible";
  vaultPath: string | null;
}

export const ImageExtension = Node.create<ImageExtensionOptions>({
  name: "image",

  group: "block",

  atom: true,

  draggable: true,

  addOptions() {
    return {
      mediaLoading: "visible",
      vaultPath: null,
    };
  },

  addAttributes() {
    return {
      src: {
        default: null,
        parseHTML: (element) => {
          const src = element.getAttribute("src") ?? "";
          return isSafeImageSrc(src) ? src : null;
        },
        renderHTML: (attributes) => {
          const src =
            typeof attributes.src === "string" && isSafeImageSrc(attributes.src)
              ? attributes.src
              : null;
          return src ? { src } : {};
        },
      },
      alt: {
        default: "",
        parseHTML: (element) => element.getAttribute("alt") ?? "",
        renderHTML: (attributes) =>
          typeof attributes.alt === "string" ? { alt: attributes.alt } : {},
      },
      title: {
        default: null,
        parseHTML: (element) => element.getAttribute("title"),
        renderHTML: (attributes) =>
          typeof attributes.title === "string" && attributes.title.trim()
            ? { title: attributes.title }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "img[src]" }];
  },

  addNodeView(): NodeViewRenderer {
    return ({ node }) => {
      let currentNode = node;
      const frame = document.createElement("div");
      const img = document.createElement("img");

      const update = () => {
        const src =
          typeof currentNode.attrs.src === "string"
            ? currentNode.attrs.src
            : "";
        const alt =
          typeof currentNode.attrs.alt === "string"
            ? currentNode.attrs.alt
            : "";
        const title =
          typeof currentNode.attrs.title === "string"
            ? currentNode.attrs.title
            : "";

        frame.className = "iris-editor-media-frame";
        frame.draggable = true;
        frame.dataset.irisMediaSrc = src;
        frame.dataset.mediaKind = "image";
        img.className = "iris-editor-media-image";
        img.draggable = true;
        img.dataset.irisMediaSrc = src;
        img.setAttribute("loading", "lazy");
        img.setAttribute("decoding", "async");
        if (src && isSafeImageSrc(src)) {
          if (this.options.mediaLoading === "visible") {
            setFrameState(frame, "pending");
            img.src = renderImageSrc(src, this.options.vaultPath);
          } else {
            setFrameState(frame, "deferred");
            img.removeAttribute("src");
          }
        } else {
          setFrameState(frame, "error");
          img.removeAttribute("src");
        }
        img.alt = alt;
        if (title.trim()) {
          img.title = title;
        } else {
          img.removeAttribute("title");
        }
      };

      img.addEventListener("load", () => {
        setFrameState(frame, "loaded");
      });
      img.addEventListener("error", () => {
        setFrameState(frame, "error");
      });
      frame.appendChild(img);
      update();
      return {
        dom: frame,
        update: (updatedNode) => {
          if (updatedNode.type !== currentNode.type) return false;
          currentNode = updatedNode;
          update();
          return true;
        },
      };
    };
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "img",
      mergeAttributes({ class: "iris-editor-media-image" }, HTMLAttributes),
    ];
  },
});
