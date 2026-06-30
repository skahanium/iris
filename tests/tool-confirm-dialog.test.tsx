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

  it("renders markdown writes as a compact permission card", async () => {
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

    expect(document.body.textContent).toContain("修改笔记");
    expect(document.body.textContent).toContain("当前选区");
    expect(document.body.textContent).toContain("会直接修改当前笔记内容。");
    expect(document.body.textContent).not.toContain("Patch 审阅");
    expect(document.body.textContent).not.toContain("base_content_hash");
    expect(document.body.textContent).not.toContain("调用参数");
  });

  it("renders backend-provided confirmation progress", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-4",
            tool_call_id: "tc-4",
            tool_name: "replace_selection",
            arguments: {
              replacement: "新的段落",
            },
            pendingConfirmationIndex: 2,
            pendingConfirmationCount: 3,
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("确认进度");
    expect(document.body.textContent).toContain("2 / 3");
  });

  it("renders credential existence checks without exposing secrets", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-secret-1",
            tool_call_id: "tc-secret-1",
            tool_name: "secret_exists",
            arguments: {
              name: "llm.deepseek",
              plaintext: "sk-should-not-render",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("检查凭据");
    expect(document.body.textContent).toContain("llm.deepseek");
    expect(document.body.textContent).toContain(
      "只检查是否存在，不会读取明文。",
    );
    expect(document.body.textContent).not.toContain("sk-should-not-render");
  });
});
