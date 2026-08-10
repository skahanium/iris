import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { useAssistantContextScope } from "@/components/ai/hooks/useAssistantContextScope";
import type { DisplayMention, SecurityDomain } from "@/types/ai";
import type { FileListItem, TagGroup } from "@/types/ipc";

const files: FileListItem[] = [
  {
    path: "Policies/Guide.md",
    title: "Guide",
    updatedAt: "2026-01-01",
    isLocked: false,
  },
];

type HookApi = ReturnType<typeof useAssistantContextScope>;

function Harness({
  onReady,
  onInput,
  loadVaultFiles,
  loadVaultFolders,
  loadVaultTags,
  domain,
}: {
  onReady: (api: HookApi) => void;
  onInput: (next: string | ((previous: string) => string)) => void;
  loadVaultFiles: () => Promise<FileListItem[]>;
  loadVaultFolders: () => Promise<string[]>;
  loadVaultTags: () => Promise<TagGroup[]>;
  domain: SecurityDomain;
}) {
  const api = useAssistantContextScope({
    setInput: onInput,
    domain,
    loadVaultFiles,
    loadVaultFolders,
    loadVaultTags,
  });
  onReady(api);
  return null;
}

describe("useAssistantContextScope", () => {
  let root: Root;
  let container: HTMLDivElement;
  let input: string;
  let api!: HookApi;
  let loadVaultFiles: () => Promise<FileListItem[]>;
  let loadVaultFolders: () => Promise<string[]>;
  let loadVaultTags: () => Promise<TagGroup[]>;
  let domain: SecurityDomain;

  function render() {
    root.render(
      createElement(Harness, {
        onReady: (next) => {
          api = next;
        },
        onInput: (next) => {
          input = typeof next === "function" ? next(input) : next;
        },
        loadVaultFiles,
        loadVaultFolders,
        loadVaultTags,
        domain,
      }),
    );
  }

  beforeEach(async () => {
    input = "";
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    loadVaultFiles = async () => files;
    loadVaultFolders = async () => ["Empty Folder/"];
    loadVaultTags = async () => [{ name: "project", files: [files[0]!] }];
    domain = "normal";
    await act(async () => {
      render();
      await Promise.resolve();
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("loads files, empty folders and tags through one candidate source", async () => {
    await act(async () => {
      await Promise.resolve();
    });
    expect(
      api
        .getMentionCandidates("@", "empty")
        .some((candidate) => candidate.value === "Empty Folder/"),
    ).toBe(true);
    expect(api.getMentionCandidates("@", "guid")[0]?.value).toBe(
      "Policies/Guide.md",
    );
    expect(api.getMentionCandidates("#", "pro")[0]?.kind).toBe("tag");
  });

  it("accepts the Composer projection without range reconciliation", () => {
    const mentions: DisplayMention[] = [
      {
        kind: "file",
        value: "Policies/Guide.md",
        label: "Guide",
        range: { from: 4, to: 9 },
      },
    ];
    act(() => {
      api.handleInputChange("ask Guide 继续", mentions);
    });
    expect(input).toBe("ask Guide 继续");
    expect(api.displayMentions).toEqual(mentions);
  });

  it("keeps retrieval scope separate from file display mentions", () => {
    act(() => {
      api.handleInputChange("ask Guide project", [
        {
          kind: "file",
          value: "Policies/Guide.md",
          label: "Guide",
          range: { from: 4, to: 9 },
        },
        {
          kind: "folder",
          value: "Empty Folder/",
          label: "Empty Folder",
          range: { from: 10, to: 21 },
        },
        {
          kind: "tag",
          value: "project",
          label: "project",
          range: { from: 22, to: 29 },
        },
      ]);
    });
    expect(api.retrievalScope).toEqual({
      paths: [],
      pathPrefixes: ["Empty Folder/"],
      requiredTags: ["project"],
    });
  });

  it("isolates display mentions by security domain", () => {
    const mention: DisplayMention = {
      kind: "file",
      value: "Policies/Guide.md",
      label: "Guide",
      range: { from: 0, to: 5 },
    };
    act(() => {
      api.handleInputChange("Guide", [mention]);
    });
    expect(api.displayMentions).toEqual([mention]);

    domain = "classified";
    act(render);
    expect(api.displayMentions).toEqual([]);

    domain = "normal";
    act(render);
    expect(api.displayMentions).toEqual([mention]);
  });
});
