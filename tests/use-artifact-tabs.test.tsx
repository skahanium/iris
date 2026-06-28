import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useArtifactTabs } from "@/hooks/useArtifactTabs";

type HookApi = ReturnType<typeof useArtifactTabs>;

function Harness({ onReady }: { onReady: (api: HookApi) => void }) {
  const api = useArtifactTabs();
  onReady(api);
  return null;
}

describe("useArtifactTabs", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;

  beforeEach(() => {
    window.localStorage.clear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(
        createElement(Harness, {
          onReady: (value) => {
            api = value;
          },
        }),
      );
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    window.localStorage.clear();
  });

  it("closes only evidence detail tabs for the deleted session", () => {
    act(() => {
      api.openArtifact({
        kind: "session_evidence_detail",
        title: "Evidence 42",
        sourceRequestId: "42",
        payload: { sessionId: 42, evidence: [] },
        persistent: false,
      });
      api.openArtifact({
        kind: "session_evidence_detail",
        title: "Evidence 7",
        sourceRequestId: "7",
        payload: { sessionId: 7, evidence: [] },
        persistent: false,
      });
      api.openArtifact({
        kind: "task_process",
        title: "Task",
        sourceRequestId: "task-1",
        payload: { markdown: "body" },
      });
    });

    act(() => {
      api.closeEvidenceArtifactsForSession(42);
    });

    expect(api.artifactTabs.map((tab) => tab.title)).toEqual([
      "Evidence 7",
      "Task",
    ]);
  });
});
