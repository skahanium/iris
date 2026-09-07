import { Extension } from "@tiptap/core";
import { Plugin, PluginKey, Selection } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";
import type { Transaction } from "@tiptap/pm/state";

import { vaultAssetImportUrl, vaultAssetWrite } from "@/lib/ipc";

const pluginKey = new PluginKey<object>("editorImageDrop");

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

async function saveImageFile(
  file: File,
  isCurrent: () => boolean,
): Promise<string | null> {
  if (!file.type.startsWith("image/")) return null;
  if (file.type === "image/svg+xml") return null;
  const ext = extensionFromMime(file.type);
  const name = `assets/${crypto.randomUUID()}.${ext}`;
  const dataBase64 = await fileToBase64(file);
  if (!isCurrent()) return null;
  return vaultAssetWrite({ path: name, dataBase64 });
}

export async function localizeRemoteImagesInHtml(
  html: string,
  isCurrent: () => boolean = () => true,
  onError?: () => void,
): Promise<string | null> {
  if (typeof DOMParser === "undefined") return null;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll("img")).filter((img) => {
    const src = img.getAttribute("src") ?? "";
    return /^https:\/\//i.test(src.trim());
  });
  if (images.length === 0) return null;

  for (const img of images) {
    if (!isCurrent()) return null;
    const src = img.getAttribute("src");
    if (!src) continue;
    try {
      const local = await vaultAssetImportUrl(src);
      img.setAttribute("src", local);
    } catch {
      img.remove();
      if (isCurrent()) onError?.();
    }
  }
  return isCurrent() ? doc.body.innerHTML : null;
}

export interface EditorImageDropOptions {
  canMutate: () => boolean;
  enabled: boolean;
  onError?: () => void;
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
      const trackSelection = (view: EditorView, position?: number) => {
        // A baseline reset creates new plugin state; ordinary transactions keep
        // this token and map the original bookmark, never the later caret.
        const generation = pluginKey.getState(view.state);
        let bookmark = (
          position === undefined
            ? view.state.selection
            : Selection.near(view.state.doc.resolve(position))
        ).getBookmark();
        const isCurrent = () =>
          !view.isDestroyed &&
          view.editable &&
          canMutate() &&
          pluginKey.getState(view.state) === generation;
        const onTransaction = ({
          transaction,
        }: {
          transaction: Transaction;
        }) => {
          if (pluginKey.getState(view.state) === generation)
            bookmark = bookmark.map(transaction.mapping);
        };
        this.editor.on("transaction", onTransaction);
        return {
          isCurrent,
          selection: () => bookmark.resolve(view.state.doc),
          release: () => {
            this.editor.off("transaction", onTransaction);
          },
        };
      };

      return [
        new Plugin({
          key: pluginKey,
          state: {
            init: () => ({}),
            apply: (_transaction, generation) => generation,
          },
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
              const pending = trackSelection(
                view,
                coords?.pos ?? view.state.selection.from,
              );
              void saveImageFile(file, pending.isCurrent)
                .then((src) => {
                  if (!src || !pending.isCurrent()) return;
                  const pos = pending.selection().from;
                  view.dispatch(
                    view.state.tr.insert(
                      pos,
                      view.state.schema.nodes.image?.create({
                        src,
                        alt: file.name.replace(/\.[^.]+$/, ""),
                      }) ?? [],
                    ),
                  );
                })
                .catch(() => this.options.onError?.())
                .finally(pending.release);
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
                  const pending = trackSelection(view);
                  void saveImageFile(file, pending.isCurrent)
                    .then((src) => {
                      if (!src || !pending.isCurrent()) return;
                      view.dispatch(
                        view.state.tr
                          .setSelection(pending.selection())
                          .replaceSelectionWith(
                            view.state.schema.nodes.image?.create({
                              src,
                              alt: file.name.replace(/\.[^.]+$/, ""),
                            }) ?? view.state.schema.text(file.name),
                          ),
                      );
                    })
                    .catch(() => this.options.onError?.())
                    .finally(pending.release);
                  return true;
                }
              }

              const html = event.clipboardData?.getData("text/html");
              if (html && /<img[^>]+src=["']https:\/\//i.test(html)) {
                event.preventDefault();
                const pending = trackSelection(view);
                void localizeRemoteImagesInHtml(
                  html,
                  pending.isCurrent,
                  this.options.onError,
                )
                  .then((localHtml) => {
                    if (!localHtml || !pending.isCurrent()) return;
                    view.dispatch(
                      view.state.tr.setSelection(pending.selection()),
                    );
                    // A fresh synthetic paste has no original remote clipboard
                    // payload, so localization cannot recursively download again.
                    view.pasteHTML(localHtml);
                  })
                  .catch(() => this.options.onError?.())
                  .finally(pending.release);
                return true;
              }

              return false;
            },
          },
        }),
      ];
    },
  });
