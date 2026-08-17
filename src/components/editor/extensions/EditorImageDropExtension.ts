import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

import { vaultAssetImportUrl, vaultAssetWrite } from "@/lib/ipc";

const pluginKey = new PluginKey("editorImageDrop");

function extensionFromMime(mime: string): string {
  const map: Record<string, string> = {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/jpg": "jpg",
    "image/gif": "gif",
    "image/webp": "webp",
  };
  return map[mime] ?? "png";
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("Failed to read image"));
        return;
      }
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () =>
      reject(reader.error ?? new Error("Failed to read image"));
    reader.readAsDataURL(file);
  });
}

async function saveImageFile(file: File): Promise<string | null> {
  if (!file.type.startsWith("image/")) return null;
  if (file.type === "image/svg+xml") return null;
  const ext = extensionFromMime(file.type);
  const name = `assets/${crypto.randomUUID()}.${ext}`;
  const dataBase64 = await fileToBase64(file);
  return vaultAssetWrite({ path: name, dataBase64 });
}

export async function localizeRemoteImagesInHtml(
  html: string,
): Promise<string | null> {
  if (typeof DOMParser === "undefined") return null;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll("img")).filter((img) => {
    const src = img.getAttribute("src") ?? "";
    return /^https:\/\//i.test(src.trim());
  });
  if (images.length === 0) return null;

  for (const img of images) {
    const src = img.getAttribute("src");
    if (!src) continue;
    try {
      const local = await vaultAssetImportUrl(src);
      img.setAttribute("src", local);
    } catch {
      img.remove();
    }
  }
  return doc.body.innerHTML;
}

export interface EditorImageDropOptions {
  canMutate: () => boolean;
  enabled: boolean;
}

/**
 * Drop / paste images into the editor → vault `assets/` + TipTap image node.
 */
export const EditorImageDropExtension =
  Extension.create<EditorImageDropOptions>({
    name: "editorImageDrop",

    addOptions() {
      return { canMutate: () => true, enabled: true };
    },

    addProseMirrorPlugins() {
      const enabled = this.options.enabled;
      const canMutate = this.options.canMutate;

      return [
        new Plugin({
          key: pluginKey,
          props: {
            handleDrop: (view, event, _slice, moved) => {
              if (
                !enabled ||
                !view.editable ||
                !canMutate() ||
                moved ||
                !event.dataTransfer?.files?.length
              ) {
                return false;
              }
              const file = Array.from(event.dataTransfer.files).find((f) =>
                f.type.startsWith("image/"),
              );
              if (!file) return false;
              event.preventDefault();
              const coords = view.posAtCoords({
                left: event.clientX,
                top: event.clientY,
              });
              void saveImageFile(file).then((src) => {
                if (!src || !view.editable || !canMutate()) return;
                const pos = coords?.pos ?? view.state.selection.from;
                view.dispatch(
                  view.state.tr.insert(
                    pos,
                    view.state.schema.nodes.image?.create({
                      src,
                      alt: file.name.replace(/\.[^.]+$/, ""),
                    }) ?? [],
                  ),
                );
              });
              return true;
            },
            handlePaste: (view, event) => {
              if (!enabled || !view.editable || !canMutate()) return false;
              const items = event.clipboardData?.items;
              if (items) {
                const fileItem = Array.from(items).find(
                  (item) =>
                    item.kind === "file" && item.type.startsWith("image/"),
                );
                const file = fileItem?.getAsFile();
                if (file) {
                  event.preventDefault();
                  const pos = view.state.selection.from;
                  void saveImageFile(file).then((src) => {
                    if (!src || !view.editable || !canMutate()) return;
                    view.dispatch(
                      view.state.tr.insert(
                        pos,
                        view.state.schema.nodes.image?.create({
                          src,
                          alt: file.name.replace(/\.[^.]+$/, ""),
                        }) ?? [],
                      ),
                    );
                  });
                  return true;
                }
              }

              const html = event.clipboardData?.getData("text/html");
              if (html && /<img[^>]+src=["']https:\/\//i.test(html)) {
                event.preventDefault();
                void localizeRemoteImagesInHtml(html).then((localHtml) => {
                  if (!localHtml || !view.editable || !canMutate()) return;
                  view.pasteHTML(localHtml, event);
                });
                return true;
              }

              return false;
            },
          },
        }),
      ];
    },
  });
