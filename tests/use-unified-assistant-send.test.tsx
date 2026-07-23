import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  useUnifiedAssistantSend,
  type UnifiedAssistantSendOptions,
} from "@/components/ai/hooks/useUnifiedAssistantSend";
import type { DisplayMention } from "@/types/ai";

const start = vi.fn();
const getFileSignature = vi.fn();
let api: ReturnType<typeof useUnifiedAssistantSend> | null = null;
let root: Root | null = null;
let host: HTMLDivElement | null = null;

const guideMention: DisplayMention = {
  kind: "file",
  value: "notes/Guide.md",
  label: "Guide",
  range: { from: 4, to: 9 },
};

function normalOptions(
  overrides: Partial<UnifiedAssistantSendOptions> = {},
): UnifiedAssistantSendOptions {
  return {
    aiDomain: "normal",
    input: "请总结 Guide",
    images: [],
    composerDisabled: false,
    session: { domain: "normal", sessionKey: "session-1" },
    contextReferences: [
      {
        id: "selection-ref",
        kind: "selection",
        filePath: "notes/source.md",
        contentHash: "selection-hash",
        utf8Range: { start: 0, end: 4 },
        editorRange: null,
        excerpt: "",
        stale: false,
      },
    ],
    displayMentions: [guideMention],
    retrievalScope: { paths: [], pathPrefixes: [], requiredTags: [] },
    webSearch: false,
    start,
    getFileSignature,
    commitAcceptedTurn: vi.fn(),
    clearContextReferences: vi.fn(),
    setInput: vi.fn(),
    setImages: vi.fn(),
    setSession: vi.fn(),
    setStreaming: vi.fn(),
    setActivityHint: vi.fn(),
    setError: vi.fn(),
    ...overrides,
  };
}

function Probe({ options }: { options: UnifiedAssistantSendOptions }) {
  api = useUnifiedAssistantSend(options);
  return null;
}

function renderProbe(options: UnifiedAssistantSendOptions) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root?.render(<Probe options={options} />));
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  api = null;
  start.mockReset();
  getFileSignature.mockReset();
});

describe("useUnifiedAssistantSend", () => {
  it("consumes one editor selection reference after adding it to one normal-domain Run", async () => {
    const consumeOneShotContextReference = vi.fn();
    const reference = normalOptions().contextReferences[0]!;
    start.mockResolvedValue({
      runId: "run-one-shot",
      turnId: "turn-one-shot",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        oneShotContextReference: reference,
        consumeOneShotContextReference,
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      reference,
    ]);
    expect(consumeOneShotContextReference).toHaveBeenCalledTimes(1);
  });

  it("does not repeat a consumed editor selection reference on the next Run", async () => {
    const reference = normalOptions().contextReferences[0]!;
    const options = normalOptions({
      contextReferences: [],
      displayMentions: [],
    });
    function StatefulProbe() {
      const [oneShotReference, setOneShotReference] = useState<
        typeof reference | null
      >(reference);
      api = useUnifiedAssistantSend({
        ...options,
        oneShotContextReference: oneShotReference,
        consumeOneShotContextReference: () => setOneShotReference(null),
      });
      return null;
    }
    start.mockResolvedValue({
      runId: "run-one-shot",
      turnId: "turn-one-shot",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<StatefulProbe />));

    await act(async () => api?.send());
    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      reference,
    ]);
    expect(start.mock.calls[1]?.[0].turn.explicitReferences).toEqual([]);
  });

  it("builds a nested normal-domain turn with a backend-compatible note hash", async () => {
    getFileSignature.mockResolvedValue({
      byteLength: 128,
      contentHash: "backend-content-hash",
      isLocked: false,
      modifiedMs: 42,
    });
    start.mockResolvedValue({
      runId: "run-1",
      turnId: "turn-1",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    renderProbe(normalOptions());

    await act(async () => api?.send());

    expect(getFileSignature).toHaveBeenCalledWith("notes/Guide.md");
    expect(start).toHaveBeenCalledWith({
      clientRequestId: expect.any(String),
      session: { domain: "normal", sessionKey: "session-1" },
      turn: {
        message: "请总结 Guide",
        explicitReferences: [
          expect.objectContaining({ id: "selection-ref" }),
          {
            id: expect.any(String),
            kind: "note",
            filePath: "notes/Guide.md",
            contentHash: "backend-content-hash",
            utf8Range: null,
            editorRange: null,
            excerpt: "",
            stale: false,
          },
        ],
        retrievalScope: {
          paths: [],
          pathPrefixes: [],
          requiredTags: [],
        },
        displayMentions: [guideMention],
      },
      webEnabled: false,
      securityDomain: "normal",
    });
  });

  it("sends folder and tag mentions only as retrieval scope", async () => {
    start.mockResolvedValue({
      runId: "run-2",
      turnId: "turn-2",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    const displayMentions: DisplayMention[] = [
      {
        kind: "folder",
        value: "Research/Notes/",
        label: "Notes",
        range: { from: 2, to: 7 },
      },
      {
        kind: "tag",
        value: "project",
        label: "project",
        range: { from: 8, to: 15 },
      },
    ];
    renderProbe(
      normalOptions({
        input: "查 Notes project",
        contextReferences: [],
        displayMentions,
        retrievalScope: {
          paths: [],
          pathPrefixes: ["Research/Notes/"],
          requiredTags: ["project"],
        },
      }),
    );

    await act(async () => api?.send());

    expect(getFileSignature).not.toHaveBeenCalled();
    expect(start.mock.calls[0]?.[0].turn).toEqual({
      message: "查 Notes project",
      explicitReferences: [],
      retrievalScope: {
        paths: [],
        pathPrefixes: ["Research/Notes/"],
        requiredTags: ["project"],
      },
      displayMentions,
    });
  });

  it("does not create transcript slots when a mentioned file cannot be signed", async () => {
    const commitAcceptedTurn = vi.fn();
    getFileSignature.mockRejectedValue(new Error("file disappeared"));
    renderProbe(normalOptions({ commitAcceptedTurn }));

    await act(async () => api?.send());

    expect(start).not.toHaveBeenCalled();
    expect(commitAcceptedTurn).not.toHaveBeenCalled();
  });

  it("does not create transcript slots when Run acceptance fails", async () => {
    const commitAcceptedTurn = vi.fn();
    const consumeOneShotContextReference = vi.fn();
    const oneShotContextReference = normalOptions().contextReferences[0]!;
    start.mockRejectedValue(new Error("agent_run_persistence_failed"));
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        oneShotContextReference,
        consumeOneShotContextReference,
        commitAcceptedTurn,
      }),
    );

    await act(async () => api?.send());

    expect(start).toHaveBeenCalledTimes(1);
    expect(commitAcceptedTurn).not.toHaveBeenCalled();
    expect(consumeOneShotContextReference).not.toHaveBeenCalled();
  });

  it("accepts at most one Run when send is invoked twice in the same tick", async () => {
    start.mockResolvedValue({
      runId: "run-double-click",
      turnId: "turn-double-click",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 0,
    });
    renderProbe(normalOptions({ contextReferences: [], displayMentions: [] }));

    await act(async () => {
      await Promise.all([api?.send(), api?.send()]);
    });

    expect(start).toHaveBeenCalledTimes(1);
  });

  it("requires a one-request classified attachment before dispatch", async () => {
    const setError = vi.fn();
    renderProbe(
      normalOptions({
        aiDomain: "classified",
        classifiedContextRef: "opaque-current-document-context",
        includeCurrentClassifiedDocument: false,
        input: "分析当前文档",
        contextReferences: [],
        displayMentions: [],
        session: null,
        setError,
      }),
    );

    await act(async () => api?.send());

    expect(start).not.toHaveBeenCalled();
    expect(setError).toHaveBeenCalledWith(
      expect.stringContaining("引用当前涉密文档"),
    );
  });

  it("rejects display mentions and retrieval scope in classified requests", async () => {
    const setError = vi.fn();
    renderProbe(
      normalOptions({
        aiDomain: "classified",
        classifiedContextRef: "opaque-current-document-context",
        includeCurrentClassifiedDocument: true,
        input: "分析 Guide",
        contextReferences: [],
        displayMentions: [{ ...guideMention, range: { from: 3, to: 8 } }],
        retrievalScope: {
          paths: [],
          pathPrefixes: ["notes/"],
          requiredTags: [],
        },
        session: null,
        setError,
      }),
    );

    await act(async () => api?.send());

    expect(start).not.toHaveBeenCalled();
    expect(setError).toHaveBeenCalledWith(expect.stringContaining("其他引用"));
  });

  it("commits Chinese fullwidth-parenthesis file mentions into the transcript", async () => {
    const label = "问题线索工作思路（王Y）";
    const input = `你如何看待 ${label} 中反映的这些线索？`;
    const from = input.indexOf(label);
    const displayMentions: DisplayMention[] = [
      {
        kind: "file",
        value: "线索/问题线索工作思路（王Y）.md",
        label,
        range: { from, to: from + label.length },
      },
    ];
    const commitAcceptedTurn = vi.fn();
    start.mockResolvedValue({
      runId: "run-zh-mention",
      turnId: "turn-zh-mention",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    getFileSignature.mockResolvedValue({
      path: "线索/问题线索工作思路（王Y）.md",
      contentHash: "zh-mention-hash",
    });

    renderProbe(
      normalOptions({
        input,
        contextReferences: [],
        displayMentions,
        commitAcceptedTurn,
      }),
    );

    await act(async () => api?.send());

    expect(start).toHaveBeenCalledWith(
      expect.objectContaining({
        turn: expect.objectContaining({
          message: input,
          displayMentions,
        }),
      }),
    );
    expect(commitAcceptedTurn).toHaveBeenCalledWith(
      input,
      expect.objectContaining({ runId: "run-zh-mention" }),
      [],
      displayMentions,
    );
  });
});
