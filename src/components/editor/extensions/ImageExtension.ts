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

interface ImageExtensionOptions {
  vaultPath: string | null;
}

export const ImageExtension = Node.create<ImageExtensionOptions>({
  name: "image",

  group: "block",

  atom: true,

  draggable: true,

  addOptions() {
    return {
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

        img.className = "iris-editor-media-image";
        img.draggable = true;
        if (src && isSafeImageSrc(src)) {
          img.src = renderImageSrc(src, this.options.vaultPath);
        } else {
          img.removeAttribute("src");
        }
        img.alt = alt;
        if (title.trim()) {
          img.title = title;
        } else {
          img.removeAttribute("title");
        }
      };

      update();
      return {
        dom: img,
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
