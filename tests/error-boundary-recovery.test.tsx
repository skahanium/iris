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
        <ErrorBoundary scope="AI panel">
          <ThrowingChild />
        </ErrorBoundary>,
      );
    });

    expect(host.textContent).toContain("boom");
    const irisLog = consoleError.mock.calls.find(
      (call) => call[0] === "Iris render error:",
    );
    expect(irisLog).toBeTruthy();
    expect(irisLog?.some((arg) => arg instanceof Error)).toBe(false);
    expect(irisLog?.[1]).toMatchObject({
      errorName: "Error",
      messageLength: 4,
      scope: "AI panel",
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
      return <div>recovered</div>;
    }

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI panel">
          <RecoverableChild />
        </ErrorBoundary>,
      );
    });
    shouldThrow = false;
    const retryButton = host.querySelector("button");
    expect(retryButton).not.toBeNull();
    await act(async () => {
      retryButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(host.textContent).toContain("recovered");
    expect(consoleError).toHaveBeenCalled();
  });

  it("copies bounded crash diagnostics without raw message text", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    await act(async () => {
      root.render(
        <ErrorBoundary scope="AI panel">
          <ThrowingChild />
        </ErrorBoundary>,
      );
    });

    const copyButton = host.querySelector(
      '[data-testid="error-boundary-copy-diagnostics"]',
    ) as HTMLButtonElement | null;
    expect(copyButton).not.toBeNull();

    await act(async () => {
      copyButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(writeText).toHaveBeenCalledTimes(1);
    const payload = JSON.parse(writeText.mock.calls[0]?.[0] as string) as {
      componentStack: string[];
      errorName: string;
      messageHash: string;
      messageLength: number;
      scope: string | null;
      timestamp: string;
    };
    expect(payload).toMatchObject({
      errorName: "Error",
      messageLength: 4,
      scope: "AI panel",
    });
    expect(payload.messageHash).toMatch(/^h[0-9a-f]+$/);
    expect(payload.componentStack.length).toBeGreaterThan(0);
    expect(Number.isNaN(Date.parse(payload.timestamp))).toBe(false);
    expect(JSON.stringify(payload)).not.toContain("boom");
    const irisLog = consoleError.mock.calls.find(
      (call) => call[0] === "Iris render error:",
    );
    expect(irisLog?.some((arg) => arg instanceof Error)).toBe(false);
  });
});
