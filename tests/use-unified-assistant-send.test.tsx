import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  useUnifiedAssistantSend,
  type UnifiedAssistantSendOptions,
} from "@/components/ai/hooks/useUnifiedAssistantSend";
import type { AssistantRunAccepted, DisplayMention } from "@/types/ai";
import type { EditorSelectionCandidate } from "@/types/editor-selection";

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
  it("blocks sending while the live editor selection candidate is not ready", async () => {
    const setError = vi.fn();
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "save_required",
          reference: null,
          message: "请先保存文档后再引用该选区",
        } satisfies EditorSelectionCandidate,
        setError,
      }),
    );

    await act(async () => api?.send());

    expect(start).not.toHaveBeenCalled();
    expect(setError).toHaveBeenCalledWith("请先保存文档后再引用该选区");
  });

  it("includes a ready live editor selection candidate in the next normal Run", async () => {
    const reference = normalOptions().contextReferences[0]!;
    start.mockResolvedValue({
      runId: "run-live-selection",
      turnId: "turn-live-selection",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    const consume = vi.fn();
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "ready",
          reference,
          message: null,
        } satisfies EditorSelectionCandidate,
        consumeEditorSelectionReference: consume,
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      reference,
    ]);
    expect(consume).toHaveBeenCalledTimes(1);
  });

  it("uses the ready candidate as the single source of the selection reference", async () => {
    const reference = normalOptions().contextReferences[0]!;
    const commitAcceptedTurn = vi.fn();
    start.mockResolvedValue({
      runId: "run-candidate-only",
      turnId: "turn-candidate-only",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "党的十八大",
          status: "ready",
          reference,
          message: null,
        } satisfies EditorSelectionCandidate,
        commitAcceptedTurn,
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      reference,
    ]);
    expect(commitAcceptedTurn).toHaveBeenCalledWith(
      "请总结 Guide",
      expect.objectContaining({ runId: "run-candidate-only" }),
      [],
      [],
      { preview: "党的十八大", fileName: "source.md" },
    );
  });

  it("does not route a stale ordinary selection candidate into classified Runs", async () => {
    start.mockResolvedValue({
      runId: "run-classified",
      turnId: "turn-classified",
      session: { domain: "classified", sessionKey: "classified-1" },
      state: "accepted",
      stateVersion: 1,
    });
    renderProbe(
      normalOptions({
        aiDomain: "classified",
        session: { domain: "classified", sessionKey: "classified-1" },
        contextReferences: [],
        displayMentions: [],
        classifiedContextRef: "opaque-current-document-context",
        includeCurrentClassifiedDocument: true,
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "ready",
          reference: normalOptions().contextReferences[0]!,
          message: null,
        },
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([]);
    expect(start.mock.calls[0]?.[0].turn.displayMentions).toEqual([]);
  });

  it("consumes one editor selection reference after adding it to one normal-domain Run", async () => {
    const consumeEditorSelectionReference = vi.fn();
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
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "ready",
          reference,
          message: null,
        } satisfies EditorSelectionCandidate,
        consumeEditorSelectionReference,
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      reference,
    ]);
    expect(consumeEditorSelectionReference).toHaveBeenCalledTimes(1);
  });

  it("freezes the sent candidate key while a Run is awaiting acceptance", async () => {
    let resolveStart: ((accepted: AssistantRunAccepted) => void) | undefined;
    const firstReference = normalOptions().contextReferences[0]!;
    const secondReference = {
      ...firstReference,
      id: "selection-ref-2",
      filePath: "notes/other.md",
    };
    const consumeEditorSelectionReference = vi.fn();
    let replaceCandidate:
      | ((candidate: EditorSelectionCandidate | null) => void)
      | undefined;
    const options = normalOptions({
      contextReferences: [],
      displayMentions: [],
      consumeEditorSelectionReference,
    });
    function StatefulProbe() {
      const [candidate, setCandidate] =
        useState<EditorSelectionCandidate | null>({
          key: "notes/source.md:1:4:selected",
          preview: "党的十八大",
          status: "ready",
          reference: firstReference,
          message: null,
        });
      replaceCandidate = setCandidate;
      api = useUnifiedAssistantSend({
        ...options,
        editorSelectionCandidate: candidate,
      });
      return null;
    }
    start.mockImplementation(
      () =>
        new Promise<AssistantRunAccepted>((resolve) => {
          resolveStart = resolve;
        }),
    );
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<StatefulProbe />));

    const sendPromise = api!.send();
    await vi.waitFor(() => expect(start).toHaveBeenCalledTimes(1));
    await act(async () => {
      replaceCandidate?.({
        key: "notes/other.md:1:4:selected",
        preview: "另一段文字",
        status: "ready",
        reference: secondReference,
        message: null,
      });
      resolveStart?.({
        runId: "run-frozen-selection",
        turnId: "turn-frozen-selection",
        clientRequestId: "client-frozen-selection",
        session: { domain: "normal", sessionKey: "session-1" },
        state: "accepted",
        stateVersion: 1,
      });
    });
    await act(async () => {
      await sendPromise;
    });

    expect(start.mock.calls[0]?.[0].turn.explicitReferences).toEqual([
      firstReference,
    ]);
    expect(consumeEditorSelectionReference).toHaveBeenCalledWith(
      "notes/source.md:1:4:selected",
    );
  });

  it("does not repeat a consumed editor selection reference on the next Run", async () => {
    const reference = normalOptions().contextReferences[0]!;
    const options = normalOptions({
      contextReferences: [],
      displayMentions: [],
    });
    function StatefulProbe() {
      const [candidate, setCandidate] =
        useState<EditorSelectionCandidate | null>({
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "ready",
          reference,
          message: null,
        });
      api = useUnifiedAssistantSend({
        ...options,
        editorSelectionCandidate: candidate,
        consumeEditorSelectionReference: () => setCandidate(null),
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

  it("attaches reviewed external tools to exactly one accepted normal Run", async () => {
    const clearExternalToolGrants = vi.fn();
    start.mockResolvedValue({
      runId: "run-external",
      turnId: "turn-external",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 0,
    });
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        externalToolGrants: [
          {
            bindingId: "binding-1",
            bindingConfigHash: "binding-hash-1",
          },
        ],
        clearExternalToolGrants,
      }),
    );

    await act(async () => api?.send());

    expect(start.mock.calls[0]?.[0].externalToolGrants).toEqual([
      {
        bindingId: "binding-1",
        bindingConfigHash: "binding-hash-1",
      },
    ]);
    expect(clearExternalToolGrants).toHaveBeenCalledTimes(1);
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
    const consumeEditorSelectionReference = vi.fn();
    const reference = normalOptions().contextReferences[0]!;
    start.mockRejectedValue(new Error("agent_run_persistence_failed"));
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        editorSelectionCandidate: {
          key: "notes/source.md:1:4:selected",
          preview: "selected",
          status: "ready",
          reference,
          message: null,
        } satisfies EditorSelectionCandidate,
        consumeEditorSelectionReference,
        commitAcceptedTurn,
      }),
    );

    await act(async () => api?.send());

    expect(start).toHaveBeenCalledTimes(2);
    expect(commitAcceptedTurn).not.toHaveBeenCalled();
    expect(consumeEditorSelectionReference).not.toHaveBeenCalled();
  });

  it("rebuilds file signatures and request identity after an automatic replay also fails", async () => {
    getFileSignature
      .mockResolvedValueOnce({
        path: "notes/Guide.md",
        contentHash: "first-content-hash",
        utf8Bytes: 10,
      })
      .mockResolvedValueOnce({
        path: "notes/Guide.md",
        contentHash: "second-content-hash",
        utf8Bytes: 11,
      });
    start.mockRejectedValue(new Error("transport unavailable"));
    renderProbe(normalOptions({ contextReferences: [] }));

    await act(async () => api?.send());
    await act(async () => api?.send());

    expect(start).toHaveBeenCalledTimes(4);
    expect(getFileSignature).toHaveBeenCalledTimes(2);
    expect(start.mock.calls[0]?.[0].clientRequestId).toBe(
      start.mock.calls[1]?.[0].clientRequestId,
    );
    expect(start.mock.calls[2]?.[0].clientRequestId).toBe(
      start.mock.calls[3]?.[0].clientRequestId,
    );
    expect(start.mock.calls[2]?.[0].clientRequestId).not.toBe(
      start.mock.calls[0]?.[0].clientRequestId,
    );
    expect(
      start.mock.calls[0]?.[0].turn.explicitReferences.at(-1)?.contentHash,
    ).toBe("first-content-hash");
    expect(
      start.mock.calls[2]?.[0].turn.explicitReferences.at(-1)?.contentHash,
    ).toBe("second-content-hash");
  });

  it("explains that changed explicit references must be attached again", async () => {
    const setError = vi.fn();
    start.mockRejectedValue(new Error("agent_run_explicit_reference_changed"));
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        setError,
      }),
    );

    await act(async () => api?.send());

    expect(setError).toHaveBeenLastCalledWith(
      "引用的文件已发生变化，请重新附加后再发送。",
    );
  });

  it("explains that an existing active Run must finish before another starts", async () => {
    const setError = vi.fn();
    start.mockRejectedValue(new Error("agent_run_active_run_exists"));
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        setError,
      }),
    );

    await act(async () => api?.send());

    expect(setError).toHaveBeenLastCalledWith(
      "当前会话已有任务运行，请等待、取消或完成后重试。",
    );
  });

  it("replays an uncertain acceptance once with the original client request id", async () => {
    const commitAcceptedTurn = vi.fn();
    start
      .mockRejectedValueOnce(
        new Error("transport closed after request dispatch"),
      )
      .mockResolvedValueOnce({
        runId: "run-replayed",
        turnId: "turn-replayed",
        session: { domain: "normal", sessionKey: "session-1" },
        state: "accepted",
        stateVersion: 0,
      });
    renderProbe(
      normalOptions({
        contextReferences: [],
        displayMentions: [],
        commitAcceptedTurn,
      }),
    );

    await act(async () => api?.send());

    expect(start).toHaveBeenCalledTimes(2);
    expect(start.mock.calls[1]?.[0].clientRequestId).toBe(
      start.mock.calls[0]?.[0].clientRequestId,
    );
    expect(commitAcceptedTurn).toHaveBeenCalledTimes(1);
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
