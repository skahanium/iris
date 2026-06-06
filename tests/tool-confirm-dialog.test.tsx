import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ToolConfirmDialog } from "@/components/ai/ToolConfirmDialog";

const toolAuditQuery = vi.fn();

vi.mock("@/lib/ipc", () => ({
  toolAuditQuery: (...args: unknown[]) => toolAuditQuery(...args),
}));

describe("ToolConfirmDialog", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    toolAuditQuery.mockReset();
    toolAuditQuery.mockResolvedValue([]);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("shows patch review metadata for markdown write tools", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-1",
            tool_call_id: "tc-1",
            tool_name: "replace_selection",
            arguments: {
              replacement: "新的段落",
              base_content_hash: "abc123",
              risk_level: "medium",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("Patch 审阅");
    expect(document.body.textContent).toContain("base_content_hash");
    expect(document.body.textContent).toContain("abc123");
    expect(document.body.textContent).toContain("medium");
  });
});
