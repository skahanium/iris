import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

import { vaultAssetWrite } from "@/lib/ipc";

const pluginKey = new PluginKey("editorImageDrop");

function extensionFromMime(mime: string): string {
  const map: Record<string, string> = {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/jpg": "jpg",
    "image/gif": "gif",
    "image/webp": "webp",
    "image/svg+xml": "svg",
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
  const ext = extensionFromMime(file.type);
  const name = `assets/${crypto.randomUUID()}.${ext}`;
  const dataBase64 = await fileToBase64(file);
  return vaultAssetWrite({ path: name, dataBase64 });
}

export interface EditorImageDropOptions {
  enabled: boolean;
}

/**
 * Drop / paste images into the editor → vault `assets/` + TipTap image node.
 */
export const EditorImageDropExtension =
  Extension.create<EditorImageDropOptions>({
    name: "editorImageDrop",

    addOptions() {
      return { enabled: true };
    },

    addProseMirrorPlugins() {
      const enabled = this.options.enabled;

      return [
        new Plugin({
          key: pluginKey,
          props: {
            handleDrop: (view, event, _slice, moved) => {
              if (!enabled || moved || !event.dataTransfer?.files?.length) {
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
                if (!src) return;
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
              if (!enabled) return false;
              const items = event.clipboardData?.items;
              if (!items) return false;
              const fileItem = Array.from(items).find(
                (item) =>
                  item.kind === "file" && item.type.startsWith("image/"),
              );
              if (!fileItem) return false;
              const file = fileItem.getAsFile();
              if (!file) return false;
              event.preventDefault();
              const pos = view.state.selection.from;
              void saveImageFile(file).then((src) => {
                if (!src) return;
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
          },
        }),
      ];
    },
  });
