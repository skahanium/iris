import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = process.cwd();

function read(path: string): string {
  return readFileSync(join(root, path), "utf8");
}

describe("阶段 0：Agent Run 契约基线", () => {
  it("将现有执行、回放与涉密边界记录为可回归的契约", () => {
    const architecture = read("ARCHITECTURE.md");
    const roadmap = read("ROADMAP.md");
    const ipcDocs = read("docs/ipc-api-reference.md");
    const routingDocs = read("docs/llm-routing.md");
    const runContract = read("src-tauri/src/ai_runtime/run_contract.rs");
    const runEngine = read("src-tauri/src/ai_runtime/run_engine.rs");
    const classifiedSession = read(
      "src-tauri/src/ai_runtime/classified_session.rs",
    );

    expect(runContract).toContain("Direct");
    expect(runContract).toContain("ToolLoop");
    expect(runContract).toContain("Durable");
    expect(runEngine).toContain("Effort::Durable");
    expect(classifiedSession).toContain("CEF-encrypted file");

    expect(architecture).toContain("不包含工具参数或原始输出");
    expect(architecture).toContain("Direct 与 ToolLoop 不支持进程级续跑");
    expect(architecture).toContain("MCP 当前只承载 Web capability mapping");
    expect(ipcDocs).toContain("不包含工具参数或原始输出");
    expect(ipcDocs).toContain("Direct 与 ToolLoop 不支持进程级续跑");
    expect(routingDocs).toContain("MCP 当前只承载 Web capability mapping");

    expect(roadmap).toContain("六阶段受控演进验收矩阵");
    expect(roadmap).toContain("阶段 0：契约校准与回归基线");
    expect(roadmap).toContain("阶段 5：可信且可解释的 Skills 激活");
    expect(roadmap).toContain("不构成发布版本承诺");
  });
});
