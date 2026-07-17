import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiComposer } from "@/components/ui/ai-composer";

describe("AiComposer display mention overlay", () => {
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

  it("keeps one accessible textarea over an aria-hidden, length-identical highlight layer", async () => {
    const value = "ask Guide\nnext";
    await act(async () => {
      root.render(
        <AiComposer
          value={value}
          displayMentions={[
            {
              kind: "file",
              value: "Policies/Guide.md",
              label: "Guide",
              range: { from: 4, to: 9 },
            },
          ]}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });

    const textarea = host.querySelector("textarea");
    const layer = host.querySelector<HTMLElement>(
      '[data-testid="ai-mention-highlight-layer"]',
    );
    const mention = layer?.querySelector(".ai-composer-display-mention");
    expect(host.querySelectorAll("textarea")).toHaveLength(1);
    expect(textarea?.getAttribute("aria-label")).toBe("AI 输入");
    expect(layer?.getAttribute("aria-hidden")).toBe("true");
    expect(layer?.textContent).toBe(value);
    expect(mention?.textContent).toBe("Guide");
    expect(mention?.getAttribute("title")).toBe("文档：Policies/Guide.md");
    expect(layer?.className).toContain("z-[2]");
    expect(mention?.className).toContain("pointer-events-auto");
    expect(textarea?.className).toContain("ai-composer-textarea-with-mentions");

    textarea?.setSelectionRange(0, 0);
    const mentionMouseDown = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    });
    act(() => mention?.dispatchEvent(mentionMouseDown));
    expect(mentionMouseDown.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(textarea);
    expect(textarea?.selectionStart).toBe(9);
  });

  it("synchronizes textarea scrolling with the highlight layer", async () => {
    await act(async () => {
      root.render(
        <AiComposer
          value={`ask Guide\n${"line\n".repeat(20)}`}
          displayMentions={[
            {
              kind: "file",
              value: "Policies/Guide.md",
              label: "Guide",
              range: { from: 4, to: 9 },
            },
          ]}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });

    const textarea = host.querySelector("textarea")!;
    const layer = host.querySelector<HTMLElement>(
      '[data-testid="ai-mention-highlight-layer"]',
    )!;
    textarea.scrollTop = 24;
    textarea.scrollLeft = 3;
    act(() => textarea.dispatchEvent(new Event("scroll", { bubbles: true })));

    expect(layer.scrollTop).toBe(24);
    expect(layer.scrollLeft).toBe(3);
  });

  it("does not prevent or submit Enter while an IME composition is being confirmed", async () => {
    const onSubmit = vi.fn();
    await act(async () => {
      root.render(
        <AiComposer value="中文" onChange={vi.fn()} onSubmit={onSubmit} />,
      );
    });
    const textarea = host.querySelector("textarea")!;

    act(() =>
      textarea.dispatchEvent(
        new CompositionEvent("compositionstart", { bubbles: true }),
      ),
    );
    const confirmingEnter = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
      isComposing: true,
    });
    act(() => textarea.dispatchEvent(confirmingEnter));

    expect(confirmingEnter.defaultPrevented).toBe(false);
    expect(onSubmit).not.toHaveBeenCalled();

    act(() =>
      textarea.dispatchEvent(
        new CompositionEvent("compositionend", { bubbles: true }),
      ),
    );
    const normalEnter = new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    });
    act(() => textarea.dispatchEvent(normalEnter));

    expect(normalEnter.defaultPrevented).toBe(true);
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("reports the exact pre-edit selection transaction from native textarea input", async () => {
    const onChange = vi.fn();
    await act(async () => {
      root.render(
        <AiComposer
          value="Guide Guide"
          onChange={onChange}
          onSubmit={vi.fn()}
        />,
      );
    });
    const textarea = host.querySelector("textarea")!;
    textarea.focus();
    textarea.setSelectionRange(0, 0);
    act(() => textarea.dispatchEvent(new Event("select", { bubbles: true })));
    act(() => document.dispatchEvent(new Event("selectionchange")));
    act(() =>
      textarea.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "v",
          ctrlKey: true,
          bubbles: true,
        }),
      ),
    );

    act(() =>
      textarea.dispatchEvent(
        new InputEvent("beforeinput", {
          bubbles: true,
          cancelable: true,
          data: "Guide ",
          inputType: "insertText",
        }),
      ),
    );
    const valueSetter = Object.getOwnPropertyDescriptor(
      HTMLTextAreaElement.prototype,
      "value",
    )?.set;
    act(() => {
      valueSetter?.call(textarea, "Guide Guide Guide");
      textarea.setSelectionRange(6, 6);
      textarea.dispatchEvent(
        new InputEvent("input", {
          bubbles: true,
          data: "Guide ",
          inputType: "insertText",
        }),
      );
    });

    expect(onChange).toHaveBeenCalledWith("Guide Guide Guide", {
      from: 0,
      to: 0,
      insertedTextLength: 6,
    });
  });
});
