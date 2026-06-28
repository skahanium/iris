import { mergeAttributes, Node, type NodeViewRenderer } from "@tiptap/core";

import { mediaRelease, mediaResolve } from "@/lib/ipc";
import { classifyWorkspacePath, type MediaKind } from "@/lib/media-reference";

interface WikiMediaEmbedOptions {
  mediaLoading: "deferred" | "visible";
  vaultPath: string | null;
}

function isSafeMediaTarget(target: string): boolean {
  const trimmed = target.trim();
  if (!trimmed) return false;
  if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) return false;
  if (trimmed.startsWith("/") || trimmed.includes("..")) return false;
  return true;
}

function labelFor(target: string, alias: string | null): string {
  return alias?.trim() || target.split(/[\\/]/).pop() || target;
}

export const WikiMediaEmbedExtension = Node.create<WikiMediaEmbedOptions>({
  name: "wikiMediaEmbed",

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
      target: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-target") ?? "",
        renderHTML: (attributes) =>
          typeof attributes.target === "string"
            ? { "data-target": attributes.target }
            : {},
      },
      alias: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-alias"),
        renderHTML: (attributes) =>
          typeof attributes.alias === "string" && attributes.alias.trim()
            ? { "data-alias": attributes.alias }
            : {},
      },
      mediaKind: {
        default: "image",
        parseHTML: (element) => element.getAttribute("data-media-kind"),
        renderHTML: (attributes) =>
          typeof attributes.mediaKind === "string"
            ? { "data-media-kind": attributes.mediaKind }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "div[data-type='wiki-media-embed']" }];
  },

  addNodeView(): NodeViewRenderer {
    return ({ node }) => {
      let currentNode = node;
      let leaseHandle: string | null = null;
      let leaseGeneration = 0;
      let intersectionObserver: IntersectionObserver | null = null;
      const root = document.createElement("div");
      root.dataset.type = "wiki-media-embed";
      root.dataset.wikiMediaEmbed = "";
      root.contentEditable = "false";

      const beginRender = () => {
        leaseGeneration += 1;
        intersectionObserver?.disconnect();
        intersectionObserver = null;
        if (leaseHandle) {
          void mediaRelease(leaseHandle);
          leaseHandle = null;
        }
        return leaseGeneration;
      };

      const attachLease = (
        target: string,
        generation: number,
        onResolved: (url: string) => void,
      ) => {
        if (this.options.mediaLoading !== "visible") return;
        const resolveVisibleMedia = () => {
          void mediaResolve(target)
            .then((resolved) => {
              if (generation !== leaseGeneration) {
                void mediaRelease(resolved.handle);
                return;
              }
              leaseHandle = resolved.handle;
              onResolved(resolved.url);
            })
            .catch(() => {
              if (generation !== leaseGeneration) return;
              root.dataset.mediaError = "true";
            });
        };
        if (!("IntersectionObserver" in window)) {
          resolveVisibleMedia();
          return;
        }
        intersectionObserver = new IntersectionObserver(
          (entries) => {
            if (!entries.some((entry) => entry.isIntersecting)) return;
            intersectionObserver?.disconnect();
            intersectionObserver = null;
            resolveVisibleMedia();
          },
          { root: null, rootMargin: "600px 0px", threshold: 0.01 },
        );
        intersectionObserver.observe(root);
      };

      const update = () => {
        const generation = beginRender();
        const target =
          typeof currentNode.attrs.target === "string"
            ? currentNode.attrs.target
            : "";
        const alias =
          typeof currentNode.attrs.alias === "string"
            ? currentNode.attrs.alias
            : null;
        const mediaKind =
          typeof currentNode.attrs.mediaKind === "string"
            ? (currentNode.attrs.mediaKind as MediaKind)
            : classifyWorkspacePath(target).mediaKind;
        const label = labelFor(target, alias);

        root.className = "iris-editor-media-embed";
        root.dataset.target = target;
        root.dataset.mediaKind = mediaKind ?? "unknown";
        if (alias?.trim()) root.dataset.alias = alias;
        else delete root.dataset.alias;
        root.replaceChildren();

        if (mediaKind === "image" && isSafeMediaTarget(target)) {
          const img = document.createElement("img");
          img.className = "iris-editor-media-image";
          img.draggable = true;
          img.dataset.irisMediaSrc = target;
          img.setAttribute("loading", "lazy");
          img.setAttribute("decoding", "async");
          img.alt = label;
          root.appendChild(img);
          attachLease(target, generation, (url) => {
            img.src = url;
          });
          return;
        }

        const title = document.createElement("span");
        title.className = "iris-editor-media-embed-title";
        title.textContent = label;
        root.appendChild(title);

        if (!isSafeMediaTarget(target)) return;
        if (mediaKind === "pdf") {
          const object = document.createElement("object");
          object.className = "iris-editor-media-pdf";
          object.type = "application/pdf";
          object.ariaLabel = label;
          attachLease(target, generation, (url) => {
            object.data = url;
            root.appendChild(object);
          });
          return;
        }
        if (mediaKind === "video") {
          const video = document.createElement("video");
          video.className = "iris-editor-media-video";
          video.controls = true;
          video.preload = "metadata";
          attachLease(target, generation, (url) => {
            video.src = url;
            root.appendChild(video);
          });
        }
      };

      update();
      return {
        dom: root,
        update: (updatedNode) => {
          if (updatedNode.type !== currentNode.type) return false;
          currentNode = updatedNode;
          update();
          return true;
        },
        destroy: () => {
          beginRender();
        },
      };
    };
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(
        {
          "data-type": "wiki-media-embed",
          "data-wiki-media-embed": "",
        },
        HTMLAttributes,
      ),
    ];
  },
});
