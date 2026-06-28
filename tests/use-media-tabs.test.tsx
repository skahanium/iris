import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { mediaTabId, useMediaTabs } from "@/hooks/useMediaTabs";

type HookApi = ReturnType<typeof useMediaTabs>;

function Harness({ onReady }: { onReady: (api: HookApi) => void }) {
  const api = useMediaTabs();
  onReady(api);
  return null;
}

describe("useMediaTabs", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;

  beforeEach(() => {
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
  });

  it("opens media immediately without reading bytes", () => {
    act(() => {
      api.openMediaPath("assets/paper.pdf", "Paper");
    });

    expect(api.activeMediaTab?.id).toBe(mediaTabId("assets/paper.pdf"));
    expect(api.activeMediaTab?.mediaKind).toBe("pdf");
    expect(api.mediaTabs).toHaveLength(1);
  });

  it("deduplicates tabs and rejects notes", () => {
    act(() => {
      api.openMediaPath("assets/photo.png", "Photo");
      api.openMediaPath("assets/photo.png", "Photo");
      api.openMediaPath("notes/Plan.md", "Plan");
    });

    expect(api.mediaTabs.map((tab) => tab.path)).toEqual(["assets/photo.png"]);
    expect(api.activeMediaTab?.title).toBe("Photo");
  });
});
