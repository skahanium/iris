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

  it("shows skills_install preview when registry preview is present", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-2",
            tool_call_id: "tc-2",
            tool_name: "skills_install",
            arguments: {
              source: "registry",
              registry: "skillhub",
              path_or_url: "scrapling",
              scope: "global",
            },
            preview: {
              display_name: "Scrapling",
              resolved_source: "url",
              resolved_url:
                "https://api.skillhub.tencent.com/api/v1/skills/scrapling/file?path=SKILL.md",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("安装预览");
    expect(document.body.textContent).toContain("Scrapling");
    expect(document.body.textContent).toContain("skillhub.tencent.com");
  });

  it("shows permission effects with risk, scope, and reversible path", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-3",
            tool_call_id: "tc-3",
            tool_name: "fetch_web_page",
            arguments: {
              url: "https://example.com/docs/phase5",
            },
            permissionEffects: [
              {
                permissionName: "web.fetch",
                riskLevel: "medium",
                scopeKind: "request",
                scopeSummary: "domain: example.com",
                reversibleBy: "删除本轮网页缓存和引用草稿",
              },
            ],
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("权限影响");
    expect(document.body.textContent).toContain("web.fetch");
    expect(document.body.textContent).toContain("medium");
    expect(document.body.textContent).toContain("request");
    expect(document.body.textContent).toContain("domain: example.com");
    expect(document.body.textContent).toContain("删除本轮网页缓存和引用草稿");
  });
});
