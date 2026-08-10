import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AiComposer,
  type AssistantComposerHandle,
} from "@/components/ui/ai-composer";
import { assistantMentionPluginKeys } from "@/lib/assistant-composer-extensions";

describe("AiComposer atomic mentions", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("renders one accessible contenteditable Composer with an atomic mention node", async () => {
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value="请查 Guide"
          composerRef={composerRef}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });
    const editor = composerRef.current?.getEditor();
    expect(host.querySelectorAll("textarea")).toHaveLength(0);
    expect(host.querySelector('[contenteditable="true"]')).not.toBeNull();
    await act(async () => {
      editor?.commands.setContent(
        {
          type: "doc",
          content: [
            {
              type: "paragraph",
              content: [
                { type: "text", text: "请查 " },
                {
                  type: "assistantMention",
                  attrs: { kind: "file", value: "Guide.md", label: "Guide" },
                },
              ],
            },
          ],
        },
        true,
      );
    });
    expect(host.querySelector(".ai-composer-mention-node")).toHaveTextContent(
      "Guide",
    );
    expect(host.querySelector(".ai-composer-mention-node svg")).not.toBeNull();
  });

  it("projects atom mentions on update and clears them as a whole node", async () => {
    const onChange = vi.fn();
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value=""
          composerRef={composerRef}
          onChange={onChange}
          onSubmit={vi.fn()}
        />,
      );
    });
    const editor = composerRef.current!.getEditor()!;
    await act(async () => {
      editor.commands.setContent(
        {
          type: "doc",
          content: [
            {
              type: "paragraph",
              content: [
                { type: "text", text: "前 " },
                {
                  type: "assistantMention",
                  attrs: {
                    kind: "folder",
                    value: "Research/",
                    label: "Research",
                  },
                },
                { type: "text", text: " 后" },
              ],
            },
          ],
        },
        true,
      );
    });
    expect(onChange).toHaveBeenLastCalledWith("前 Research 后", [
      expect.objectContaining({
        kind: "folder",
        value: "Research/",
        label: "Research",
        range: { from: 2, to: 10 },
      }),
    ]);

    const nodePosition = 1 + "前 ".length;
    editor.commands.setNodeSelection(nodePosition);
    editor.commands.deleteSelection();
    expect(editor.getText()).toBe("前  后");
    let hasMention = false;
    editor.state.doc.descendants((node) => {
      if (node.type.name === "assistantMention") hasMention = true;
    });
    expect(hasMention).toBe(false);
  });

  it("does not submit Enter while the contenteditable is composing", async () => {
    const onSubmit = vi.fn();
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value="中文"
          composerRef={composerRef}
          onChange={vi.fn()}
          onSubmit={onSubmit}
        />,
      );
    });
    const editor = host.querySelector<HTMLElement>('[contenteditable="true"]')!;
    const event = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
      isComposing: true,
    });
    act(() => editor.dispatchEvent(event));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("uses the latest draft when Enter submits", async () => {
    const onSubmit = vi.fn();
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value=""
          composerRef={composerRef}
          onChange={vi.fn()}
          onSubmit={onSubmit}
        />,
      );
    });
    await act(async () => {
      root.render(
        <AiComposer
          value="新的问题"
          composerRef={composerRef}
          onChange={vi.fn()}
          onSubmit={onSubmit}
        />,
      );
    });
    const editor = host.querySelector<HTMLElement>('[contenteditable="true"]')!;
    act(() => {
      editor.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Enter",
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("closes an active mention candidate list on Escape", async () => {
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value=""
          composerRef={composerRef}
          getMentionCandidates={() => [
            {
              id: "A/Guide.md",
              kind: "file",
              value: "A/Guide.md",
              label: "Guide",
            },
          ]}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });
    const editor = composerRef.current!.getEditor()!;
    await act(async () => {
      editor.chain().focus().insertContent("@Gui").run();
    });
    expect(
      (
        assistantMentionPluginKeys.at.getState(editor.state) as
          | { active: boolean }
          | undefined
      )?.active,
    ).toBe(true);

    act(() => {
      host
        .querySelector<HTMLElement>('[contenteditable="true"]')!
        .dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "Escape",
            bubbles: true,
            cancelable: true,
          }),
        );
    });
    expect(
      (
        assistantMentionPluginKeys.at.getState(editor.state) as
          | { active: boolean }
          | undefined
      )?.active,
    ).toBe(false);
  });

  it("keeps same-name files distinct and supports a Chinese fullwidth boundary", async () => {
    const onChange = vi.fn();
    const composerRef = {
      current: null,
    } as React.MutableRefObject<AssistantComposerHandle | null>;
    await act(async () => {
      root.render(
        <AiComposer
          value=""
          composerRef={composerRef}
          getMentionCandidates={() => [
            {
              id: "A/Guide.md",
              kind: "file",
              value: "A/Guide.md",
              label: "Guide",
            },
            {
              id: "B/Guide.md",
              kind: "file",
              value: "B/Guide.md",
              label: "Guide",
            },
          ]}
          onChange={onChange}
          onSubmit={vi.fn()}
        />,
      );
    });
    const editor = composerRef.current!.getEditor()!;
    await act(async () => {
      editor.chain().focus().insertContent("（@Gui").run();
    });
    expect(
      composerRef.current?.insertMention({
        id: "B/Guide.md",
        kind: "file",
        value: "B/Guide.md",
        label: "Guide",
      }),
    ).toBe(true);
    expect(onChange).toHaveBeenLastCalledWith("（Guide", [
      expect.objectContaining({ kind: "file", value: "B/Guide.md" }),
    ]);
  });
});
