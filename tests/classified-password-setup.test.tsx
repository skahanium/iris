import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ClassifiedPasswordSetup } from "@/components/classified/ClassifiedPasswordSetup";

const classifiedSetup = vi.fn();

vi.mock("@/lib/ipc", () => ({
  classifiedSetup: (...args: unknown[]) => classifiedSetup(...args),
}));

function setInput(input: HTMLInputElement, value: string) {
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("ClassifiedPasswordSetup", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    classifiedSetup.mockReset();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("clears both password inputs when setup fails", async () => {
    classifiedSetup.mockRejectedValueOnce(new Error("setup failed"));

    await act(async () => {
      root.render(<ClassifiedPasswordSetup onSuccess={vi.fn()} />);
    });

    const inputs = Array.from(
      document.querySelectorAll<HTMLInputElement>('input[type="password"]'),
    );
    expect(inputs).toHaveLength(2);

    setInput(inputs[0]!, "very-secret");
    setInput(inputs[1]!, "very-secret");

    const submit = Array.from(document.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("确认设置"),
    );
    await act(async () => {
      submit?.click();
    });

    await vi.waitFor(() => {
      expect(classifiedSetup).toHaveBeenCalledWith("very-secret");
    });
    expect(inputs[0]?.value).toBe("");
    expect(inputs[1]?.value).toBe("");
    expect(document.body.textContent).toContain("setup failed");
  });
});
