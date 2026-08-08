import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AssistantComposerDock } from "@/components/ai/AssistantComposerDock";
import type { ContextReference } from "@/types/ai";

afterEach(cleanup);

function reference(
  overrides: Partial<ContextReference> = {},
): ContextReference {
  return {
    id: "ref-1",
    kind: "note",
    filePath: "notes/alpha.md",
    contentHash: "hash-1",
    utf8Range: { start: 0, end: 10 },
    editorRange: null,
    excerpt: "alpha beta gamma",
    stale: false,
    ...overrides,
  };
}

function renderDock({
  contextReferences = [],
  onRemoveContextReference = vi.fn(),
}: {
  contextReferences?: ContextReference[];
  onRemoveContextReference?: (id: string) => void;
} = {}) {
  return render(
    <AssistantComposerDock
      composerDisabled={false}
      images={[]}
      input=""
      displayMentions={[]}
      mentionCandidates={[]}
      mentionHighlight={0}
      mentionNavDeltaRef={{ current: 0 }}
      mentionOpen={false}
      mentionPrefix="@"
      mentionQuery=""
      streaming={false}
      externalBindings={[]}
      selectedExternalBindingIds={[]}
      contextReferences={contextReferences}
      textareaRef={{ current: null }}
      onComposerKeyDown={() => {}}
      onCompositionStart={() => {}}
      onCompositionEnd={() => {}}
      onImagesChange={() => {}}
      onExternalBindingToggle={() => {}}
      onRemoveContextReference={onRemoveContextReference}
      onMentionHighlight={() => {}}
      onMentionSelect={() => {}}
      onSubmit={() => {}}
      onValueChange={() => {}}
      onSelect={() => {}}
      onStop={() => {}}
    />,
  );
}

describe("AssistantComposerDock context references", () => {
  it("does not render the reference boundary without collected references", () => {
    renderDock();
    expect(
      screen.queryByTestId("context-reference-boundary"),
    ).not.toBeInTheDocument();
  });

  it("renders collected reference chips with their display text", () => {
    renderDock({
      contextReferences: [
        reference(),
        reference({
          id: "ref-2",
          filePath: "notes/beta.md",
          excerpt: "hello world",
          stale: true,
        }),
      ],
    });
    const boundary = screen.getByTestId("context-reference-boundary");
    expect(boundary).toBeInTheDocument();
    expect(boundary).toHaveTextContent("alpha.md");
    expect(boundary).toHaveTextContent("alpha beta gamma");
    expect(boundary).toHaveTextContent("beta.md");
    expect(boundary).toHaveTextContent("已失效");
  });

  it("removes a reference when its remove button is clicked", () => {
    const onRemoveContextReference = vi.fn();
    renderDock({
      contextReferences: [reference()],
      onRemoveContextReference,
    });
    fireEvent.click(screen.getByRole("button", { name: "移除引用 ref-1" }));
    expect(onRemoveContextReference).toHaveBeenCalledWith("ref-1");
  });
});
