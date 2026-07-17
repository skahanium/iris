import type { Editor } from "@tiptap/react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { AI_STREAM_TRANSIENT_TRANSACTION_META } from "@/components/editor/extensions/AiStreamExtension";
import { useInlineAi } from "@/hooks/useInlineAi";
import { EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE } from "@/lib/context-reference";
import {
  assistantRunStart,
  fileSignature,
  listenAssistantRunEvent,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  assistantRunControl: vi.fn(async () => undefined),
  assistantRunStart: vi.fn(),
  fileSignature: vi.fn(),
  listenAssistantRunEvent: vi.fn(async () => () => undefined),
}));

const mockAssistantRunStart = vi.mocked(assistantRunStart);
const mockFileSignature = vi.mocked(fileSignature);
const mockListenAssistantRunEvent = vi.mocked(listenAssistantRunEvent);
const markdown = "first selection";

async function signatureFor(content: string) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(content),
  );
  return {
    byteLength: new TextEncoder().encode(content).length,
    contentHash: Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join(""),
    isLocked: false,
    modifiedMs: 1,
  };
}

function acceptedRun() {
  return {
    runId: "real-inline-run",
    turnId: "real-inline-turn",
    session: { domain: "normal" as const, sessionKey: "real-inline-session" },
    state: "accepted" as const,
    stateVersion: 1,
  };
}

describe("TipTap inline AI dirty boundary", () => {
  let root: Root | null = null;
  let container: HTMLDivElement | null = null;
  let editor: Editor | null = null;
  let dirty = false;
  let status = "";
  let inlineAi: ReturnType<typeof useInlineAi>;

  function Host() {
    inlineAi = useInlineAi({
      isDocumentDirty: () => dirty,
      onStatus: (message) => {
        status = message;
      },
    });
    return (
      <TipTapEditor
        initialBodyMarkdown={markdown}
        committedSourceMarkdown={markdown}
        contentCacheKey="notes/real-inline.md"
        onContentReady={(ready) => {
          editor = ready;
        }}
        onDirty={() => {
          dirty = true;
        }}
        onInlineAiRetry={(target) => void inlineAi.retry(target)}
      />
    );
  }

  beforeEach(async () => {
    dirty = false;
    status = "";
    editor = null;
    mockAssistantRunStart.mockReset();
    mockFileSignature.mockReset();
    mockListenAssistantRunEvent.mockClear();
    mockFileSignature.mockImplementation(() => signatureFor(markdown));
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(<Host />);
      await Promise.resolve();
      await Promise.resolve();
    });
    // The application establishes a clean disk baseline after hydration.
    dirty = false;
  });

  afterEach(() => {
    if (root) act(() => root?.unmount());
    container?.remove();
    root = null;
    container = null;
    editor = null;
  });

  it("retries a failed Start with the same reference without marking transient AI UI dirty", async () => {
    expect(dirty).toBe(false);
    const updateMeta: unknown[] = [];
    editor!.on("update", ({ transaction }) => {
      updateMeta.push(
        transaction.getMeta(AI_STREAM_TRANSIENT_TRANSACTION_META),
      );
    });
    mockAssistantRunStart
      .mockRejectedValueOnce(new Error("start failed"))
      .mockResolvedValueOnce(acceptedRun());
    editor!.commands.setTextSelection({ from: 1, to: 6 });

    await act(async () => {
      await inlineAi.run(editor!, "rewrite");
    });
    expect(updateMeta).toEqual(updateMeta.map(() => true));
    expect(dirty).toBe(false);
    await act(async () => {
      await inlineAi.retry(editor!);
    });

    const firstReference =
      mockAssistantRunStart.mock.calls[0]?.[0].turn.explicitReferences[0];
    expect(mockAssistantRunStart).toHaveBeenCalledTimes(2);
    expect(
      mockAssistantRunStart.mock.calls[1]?.[0].turn.explicitReferences,
    ).toEqual([firstReference]);
  });

  it("still rejects inline AI after a real user edit", async () => {
    editor!.commands.insertContentAt(6, " user edit");
    editor!.commands.setTextSelection({ from: 1, to: 6 });

    await act(async () => {
      await inlineAi.run(editor!, "rewrite");
    });

    expect(dirty).toBe(true);
    expect(mockAssistantRunStart).not.toHaveBeenCalled();
    expect(status).toBe(EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE);
  });
});
