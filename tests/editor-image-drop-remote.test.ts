import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  EditorImageDropExtension,
  localizeRemoteImagesInHtml,
} from "@/components/editor/extensions/EditorImageDropExtension";
import { resetEditorContentBaseline } from "@/lib/editor-baseline";
import { ImageExtension } from "@/components/editor/extensions/ImageExtension";
import * as ipc from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  vaultAssetImportUrl: vi.fn(),
  vaultAssetWrite: vi.fn(),
}));

const mockedImport = vi.mocked(ipc.vaultAssetImportUrl);

beforeEach(() => {
  vi.resetAllMocks();
  // jsdom omits this browser constructor used by ProseMirror pasteHTML.
  vi.stubGlobal("ClipboardEvent", class extends Event {});
});

let editor: Editor | undefined;
afterEach(() => {
  editor?.destroy();
  editor = undefined;
  vi.unstubAllGlobals();
});

function pasteRemoteHtml(target: Editor, html: string) {
  const event = new Event("paste", { cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: {
      items: [],
      getData: (type: string) => (type === "text/html" ? html : ""),
    },
  });
  target.view.someProp("handlePaste", (handler) =>
    handler(
      target.view,
      event as ClipboardEvent,
      target.state.selection.content(),
    ),
  );
}

it("does not paste a delayed download into a reloaded document", async () => {
  let resolve!: (value: string) => void;
  mockedImport.mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  editor = new Editor({
    extensions: [StarterKit, EditorImageDropExtension],
    content: "<p>original document</p>",
  });
  pasteRemoteHtml(
    editor,
    '<p>old paste<img src="https://example.com/a.png"></p>',
  );
  resetEditorContentBaseline(editor, "<p>new document</p>");
  resolve("assets/download.png");
  await vi.waitFor(() => expect(mockedImport).toHaveBeenCalledOnce());
  await new Promise((done) => setTimeout(done, 0));
  expect(mockedImport).toHaveBeenCalledOnce();
  expect(editor.getText()).toBe("new document");
});

it("keeps an asynchronous paste at its original selection after the caret moves", async () => {
  let resolve!: (value: string) => void;
  mockedImport.mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  editor = new Editor({
    extensions: [StarterKit, EditorImageDropExtension],
    content: "<p>first</p><p>second</p>",
  });
  editor.commands.setTextSelection(1);
  pasteRemoteHtml(
    editor,
    '<span>PASTED<img src="https://example.com/a.png"></span>',
  );
  editor.commands.setTextSelection(8);
  resolve("assets/download.png");
  await vi.waitFor(() => expect(editor?.getText()).toContain("PASTED"));
  expect(editor.getText()).toBe("PASTEDfirst\n\nsecond");
  expect(mockedImport).toHaveBeenCalledOnce();
});

it("maps the pending paste through intervening document edits", async () => {
  let resolve!: (value: string) => void;
  mockedImport.mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  editor = new Editor({
    extensions: [StarterKit, EditorImageDropExtension],
    content: "<p>first</p><p>second</p>",
  });
  editor.commands.setTextSelection(8);
  pasteRemoteHtml(
    editor,
    '<span>PASTED<img src="https://example.com/a.png"></span>',
  );
  editor.commands.insertContentAt(1, "PREFIX");
  resolve("assets/download.png");
  await vi.waitFor(() => expect(editor?.getText()).toContain("PASTED"));
  expect(editor.getText()).toBe("PREFIXfirst\n\nPASTEDsecond");
});

it("does not insert a downloaded file after the document baseline changed", async () => {
  let resolve!: (value: string) => void;
  const write = vi.mocked(ipc.vaultAssetWrite);
  write.mockImplementation(
    () =>
      new Promise<string>((done) => {
        resolve = done;
      }),
  );
  editor = new Editor({
    extensions: [StarterKit, EditorImageDropExtension],
    content: "<p>original</p>",
  });
  const event = new Event("paste", { cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: {
      items: [
        {
          kind: "file",
          type: "image/png",
          getAsFile: () =>
            new File(["bytes"], "image.png", { type: "image/png" }),
        },
      ],
    },
  });
  editor.view.someProp("handlePaste", (handler) =>
    handler(
      editor!.view,
      event as ClipboardEvent,
      editor!.state.selection.content(),
    ),
  );
  await vi.waitFor(() => expect(write).toHaveBeenCalledOnce());
  resetEditorContentBaseline(editor, "<p>new document</p>");
  resolve("assets/download.png");
  await new Promise((done) => setTimeout(done, 0));
  expect(editor.getText()).toBe("new document");
});

it("reports a remote image failure without losing surrounding pasted text", async () => {
  const onError = vi.fn();
  mockedImport.mockRejectedValue(new Error("backend unavailable"));
  editor = new Editor({
    extensions: [StarterKit, EditorImageDropExtension.configure({ onError })],
    content: "<p>original</p>",
  });
  pasteRemoteHtml(
    editor,
    '<span>PASTED<img src="https://example.com/a.png"></span>',
  );
  await vi.waitFor(() => expect(editor?.getText()).toContain("PASTED"));
  expect(onError).toHaveBeenCalledOnce();
});

it("inserts the real image node once and keeps paste in editor undo", async () => {
  mockedImport.mockResolvedValue("assets/download.png");
  editor = new Editor({
    extensions: [StarterKit, ImageExtension, EditorImageDropExtension],
    content: "<p>original</p>",
  });
  pasteRemoteHtml(editor, '<img src="https://example.com/a.png" alt="photo">');
  await vi.waitFor(() =>
    expect(editor?.getHTML()).toContain('src="assets/download.png"'),
  );
  expect(mockedImport).toHaveBeenCalledOnce();
  expect(editor.getHTML().match(/<img /g)).toHaveLength(1);
  expect(editor.commands.undo()).toBe(true);
  expect(editor.getHTML()).toBe("<p>original</p>");
});

it.each(["disabled", "destroyed"] as const)(
  "ignores a pending paste after the editor is %s",
  async (state) => {
    let resolve!: (value: string) => void;
    mockedImport.mockImplementation(
      () => new Promise<string>((done) => (resolve = done)),
    );
    editor = new Editor({
      extensions: [StarterKit, ImageExtension, EditorImageDropExtension],
      content: "<p>original</p>",
    });
    pasteRemoteHtml(editor, '<img src="https://example.com/a.png">');
    const view = editor.view;
    if (state === "disabled") editor.setEditable(false);
    else editor.destroy();
    const dispatch = vi.spyOn(view, "dispatch");
    resolve("assets/download.png");
    await new Promise((done) => setTimeout(done, 0));
    expect(
      dispatch.mock.calls.some(([transaction]) => transaction.docChanged),
    ).toBe(false);
    expect(view.state.doc.textContent).toBe("original");
    expect(view.state.doc.toJSON().content).toHaveLength(1);
    expect(mockedImport).toHaveBeenCalledOnce();
    dispatch.mockRestore();
  },
);

it("maps an asynchronous image drop through edits before its original position", async () => {
  let resolve!: (value: string) => void;
  const write = vi.mocked(ipc.vaultAssetWrite);
  write.mockImplementation(
    () => new Promise<string>((done) => (resolve = done)),
  );
  editor = new Editor({
    extensions: [StarterKit, ImageExtension, EditorImageDropExtension],
    content: "<p>first</p><p>second</p>",
  });
  const position = vi
    .spyOn(editor.view, "posAtCoords")
    .mockReturnValue({ pos: 8, inside: 7 });
  const event = new Event("drop", { cancelable: true });
  Object.defineProperty(event, "dataTransfer", {
    value: { files: [new File(["bytes"], "photo.png", { type: "image/png" })] },
  });
  editor.view.someProp("handleDrop", (handler) =>
    handler(
      editor!.view,
      event as DragEvent,
      editor!.state.selection.content(),
      false,
    ),
  );
  await vi.waitFor(() => expect(write).toHaveBeenCalledOnce());
  editor.commands.insertContentAt(1, "PREFIX");
  resolve("assets/drop.png");
  await vi.waitFor(() =>
    expect(editor?.getHTML()).toContain('src="assets/drop.png"'),
  );
  const html = editor.getHTML();
  expect(html.indexOf("PREFIXfirst")).toBeLessThan(html.indexOf("<img"));
  expect(html.indexOf("<img")).toBeLessThan(html.indexOf("second"));
  expect(write).toHaveBeenCalledOnce();
  position.mockRestore();
});

describe("localizeRemoteImagesInHtml", () => {
  it("replaces remote https images with local vault asset paths", async () => {
    mockedImport.mockResolvedValue("assets/abc.png");

    const result = await localizeRemoteImagesInHtml(
      '<p>before<img src="https://example.com/a.png" alt="a">after</p>',
    );

    expect(result).toContain('src="assets/abc.png"');
    expect(result).toContain("before");
    expect(result).toContain("after");
    expect(mockedImport).toHaveBeenCalledWith("https://example.com/a.png");
  });

  it("removes images that fail to download instead of leaving broken links", async () => {
    mockedImport.mockRejectedValue(new Error("download failed"));

    const result = await localizeRemoteImagesInHtml(
      '<p><img src="https://example.com/b.png" alt="b"></p>',
    );

    expect(result).not.toContain("example.com");
    expect(result).not.toContain("<img");
  });
});
