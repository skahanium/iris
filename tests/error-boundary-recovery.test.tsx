import { readFileSync } from "node:fs";

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ErrorBoundary } from "@/components/ErrorBoundary";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function ThrowingChild() {
  throw new Error("boom");
  return null;
}

describe("ErrorBoundary recovery", () => {
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
    vi.restoreAllMocks();
  });

  it("logs sanitized render error details without passing the raw Error object", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI对话区">
          <ThrowingChild />
        </ErrorBoundary>,
      );
    });

    expect(host.textContent).toContain("界面出现异常（AI对话区）");
    const irisLog = consoleError.mock.calls.find(
      (call) => call[0] === "Iris render error:",
    );
    expect(irisLog).toBeTruthy();
    expect(irisLog?.some((arg) => arg instanceof Error)).toBe(false);
    expect(irisLog?.[1]).toMatchObject({
      errorName: "Error",
      messageLength: 4,
      scope: "AI对话区",
    });
  });

  it("uses a reset key so retry remounts the failed subtree", async () => {
    const source = read("src/components/ErrorBoundary.tsx");

    expect(source).toContain("resetVersion");
    expect(source).toContain("key={this.state.resetVersion}");

    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    let shouldThrow = true;

    function RecoverableChild() {
      if (shouldThrow) throw new Error("boom");
      return <div>已恢复</div>;
    }

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI对话区">
          <RecoverableChild />
        </ErrorBoundary>,
      );
    });
    shouldThrow = false;
    const retryButton = host.querySelector("button");
    expect(retryButton?.textContent).toBe("重试");
    await act(async () => {
      retryButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(host.textContent).toContain("已恢复");
    expect(consoleError).toHaveBeenCalled();
  });
});
