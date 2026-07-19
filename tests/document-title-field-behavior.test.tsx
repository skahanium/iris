import { act, createElement, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DocumentTitleField } from "@/components/editor/DocumentTitleField";

function TitleHarness({
  value,
  resetKey,
  onChange,
  onBlur,
}: {
  value: string;
  resetKey: string;
  onChange: (next: string) => void;
  onBlur?: (next: string) => void;
}) {
  return createElement(DocumentTitleField, {
    value,
    resetKey,
    onChange,
    onBlur,
    editorRef: { current: null },
  });
}

describe("DocumentTitleField uncontrolled behavior", () => {
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

  function textarea(): HTMLTextAreaElement {
    const el = host.querySelector('[data-testid="document-title"]');
    if (!(el instanceof HTMLTextAreaElement)) {
      throw new Error("document title textarea missing");
    }
    return el;
  }

  it("does not revert DOM while focused when parent value lags behind", async () => {
    const onChange = vi.fn();
    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "旧标题",
          resetKey: "note-a.md",
          onChange,
        }),
      );
    });

    const field = textarea();
    await act(async () => {
      field.focus();
      field.value = "Iris E2E Persistence";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    });

    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "旧标题",
          resetKey: "note-a.md",
          onChange,
        }),
      );
    });

    expect(textarea().value).toBe("Iris E2E Persistence");
  });

  it("mirrors external value into DOM when blurred", async () => {
    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "旧标题",
          resetKey: "note-a.md",
          onChange: () => undefined,
        }),
      );
    });

    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "磁盘标题",
          resetKey: "note-a.md",
          onChange: () => undefined,
        }),
      );
    });

    expect(textarea().value).toBe("磁盘标题");
  });

  it("remounts with a new title when resetKey changes", async () => {
    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "笔记 A",
          resetKey: "a.md",
          onChange: () => undefined,
        }),
      );
    });

    expect(textarea().value).toBe("笔记 A");

    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "笔记 B",
          resetKey: "b.md",
          onChange: () => undefined,
        }),
      );
    });

    expect(textarea().value).toBe("笔记 B");
  });

  it("commits DOM value on blur", async () => {
    const onBlur = vi.fn();
    await act(async () => {
      root.render(
        createElement(TitleHarness, {
          value: "旧标题",
          resetKey: "note-a.md",
          onChange: () => undefined,
          onBlur,
        }),
      );
    });

    const field = textarea();
    await act(async () => {
      field.focus();
      field.value = "Iris E2E Persistence";
      field.dispatchEvent(new Event("input", { bubbles: true }));
      field.blur();
    });

    expect(onBlur).toHaveBeenCalledWith("Iris E2E Persistence");
  });
});

function HarnessWithState({
  initialTitle,
  resetKey,
}: {
  initialTitle: string;
  resetKey: string;
}) {
  const [title, setTitle] = useState(initialTitle);
  return createElement(TitleHarness, {
    value: title,
    resetKey,
    onChange: setTitle,
  });
}

describe("DocumentTitleField parent state", () => {
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

  it("updates parent state from input events", async () => {
    await act(async () => {
      root.render(
        createElement(HarnessWithState, {
          initialTitle: "旧标题",
          resetKey: "note-a.md",
        }),
      );
    });

    const field = host.querySelector(
      '[data-testid="document-title"]',
    ) as HTMLTextAreaElement;
    await act(async () => {
      field.focus();
      field.value = "新标题";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    });

    const updated = host.querySelector(
      '[data-testid="document-title"]',
    ) as HTMLTextAreaElement;
    expect(updated.value).toBe("新标题");
  });
});
