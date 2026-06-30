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

  it("renders skill installs without raw tool details", async () => {
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
              target_install_dir: "D:/vault/.iris/skills",
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

    expect(document.body.textContent).toContain("安装 Skill");
    expect(document.body.textContent).toContain("Scrapling");
    expect(document.body.textContent).toContain("D:/vault/.iris/skills");
    expect(document.body.textContent).toContain(
      "会把 Skill 安装到指定目录，并在当前会话中可用。",
    );
    expect(document.body.textContent).not.toContain("skills_install");
    expect(document.body.textContent).not.toContain("resolved_url");
    expect(document.body.textContent).not.toContain("调用参数");
  });

  it("renders web fetches as a short user-facing approval", async () => {
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

    expect(document.body.textContent).toContain("读取网页内容");
    expect(document.body.textContent).toContain("example.com");
    expect(document.body.textContent).toContain("/docs/phase5");
    expect(document.body.textContent).toContain(
      "会向该网站发送一次请求，网页内容会进入当前对话。",
    );
    expect(document.body.textContent).toContain("拒绝");
    expect(document.body.textContent).toContain("允许");
    expect(document.body.textContent).not.toContain("权限影响");
    expect(document.body.textContent).not.toContain("web.fetch");
    expect(document.body.textContent).not.toContain("medium");
    expect(document.body.textContent).not.toContain("request");
    expect(document.body.textContent).not.toContain("修改参数");
    expect(document.body.textContent).not.toContain("调用参数");
  });

  it("renders backend-provided confirmation progress", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-4",
            tool_call_id: "tc-4",
            tool_name: "fetch_web_page",
            arguments: {
              url: "https://example.com/second",
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
  it("renders MCP profile upsert with transport scope and capability boundary", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-mcp-1",
            tool_call_id: "tc-mcp-1",
            tool_name: "mcp_runtime_profile_upsert",
            arguments: {
              id: "anysearch-local",
              server_id: "anysearch",
              display_name: "AnySearch Local",
              enabled: true,
              transport_config_json:
                '{"type":"stdio","command":"anysearch-mcp"}',
              env_bindings_json:
                '{"ANYSEARCH_API_KEY":"credential://anysearch"}',
            },
            preview: {
              operation: "mcp_profile_upsert",
              profile_id: "anysearch-local",
              server_id: "anysearch",
              display_name: "AnySearch Local",
              transport: "stdio",
              vault_scope: "vault-abc",
              credential_bindings: 1,
              starts_process: false,
              capability_boundary: "controlled_iris_capability_mapping",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("注册 MCP Profile");
    expect(document.body.textContent).toContain("AnySearch Local");
    expect(document.body.textContent).toContain("stdio");
    expect(document.body.textContent).toContain("vault-abc");
    expect(document.body.textContent).toContain("Iris capability mapping");
    expect(document.body.textContent).toContain("不会启动本地进程");
    expect(document.body.textContent).not.toContain("credential://anysearch");
  });

  it("renders MCP server catalog upsert without raw secret config", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-mcp-server-1",
            tool_call_id: "tc-mcp-server-1",
            tool_name: "mcp_server_catalog_upsert",
            arguments: {
              id: "anysearch",
              display_name: "AnySearch",
              transport: "stdio",
              command: "anysearch-mcp",
              env_schema_json:
                '{"ANYSEARCH_API_KEY":{"description":"credential://anysearch"}}',
            },
            preview: {
              operation: "mcp_server_catalog_upsert",
              server_id: "anysearch",
              display_name: "AnySearch",
              transport: "stdio",
              source: "user",
              starts_process: false,
              capability_boundary: "controlled_iris_capability_mapping",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("注册 MCP Server");
    expect(document.body.textContent).toContain("AnySearch");
    expect(document.body.textContent).toContain("transport: stdio");
    expect(document.body.textContent).toContain("不会启动本地进程");
    expect(document.body.textContent).not.toContain("credential://anysearch");
  });

  it("renders live MCP tools discovery as a bounded local process", async () => {
    await act(async () => {
      root.render(
        <ToolConfirmDialog
          request={{
            request_id: "req-mcp-2",
            tool_call_id: "tc-mcp-2",
            tool_name: "mcp_runtime_tools_list",
            arguments: {
              profile_id: "anysearch-local",
              reason: "discover provider tools",
            },
            preview: {
              operation: "mcp_tools_list",
              profile_id: "anysearch-local",
              starts_process: true,
              process_kind: "bounded_stdio_mcp",
              result_scope: "sanitized_tool_inventory",
              reason: "discover provider tools",
            },
          }}
          onConfirm={() => {}}
          onClose={() => {}}
        />,
      );
    });

    expect(document.body.textContent).toContain("发现 MCP 工具");
    expect(document.body.textContent).toContain("anysearch-local");
    expect(document.body.textContent).toContain("会启动受控本地 MCP 进程");
    expect(document.body.textContent).toContain("sanitized_tool_inventory");
    expect(document.body.textContent).toContain("discover provider tools");
  });
});
