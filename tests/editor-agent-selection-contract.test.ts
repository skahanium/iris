import { readFileSync } from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { filterEditorActions } from "@/lib/editor-actions";

const repoRoot = process.cwd();

function readRepoFile(relativePath: string): string {
  return readFileSync(path.join(repoRoot, relativePath), "utf8");
}

describe("editor ↔ Agent temporary selection contract", () => {
  it("documents live-linked selection semantics without implicit document input", () => {
    const designSystem = readRepoFile("docs/design-system.md");
    const adaptiveWorkspace = readRepoFile("docs/adaptive-workspace.md");
    const roadmap = readRepoFile("ROADMAP.md");

    for (const document of [designSystem, adaptiveWorkspace, roadmap]) {
      expect(document).toContain("文档与 Agent 默认");
      expect(document).toContain("非空文字选区");
      expect(document).toContain("切换文档");
      expect(document).toMatch(/(?:立即|即刻)解除/u);
      expect(document).toMatch(/锁定(?:的)?普通文档/u);
      expect(document).toContain("classified");
    }

    expect(adaptiveWorkspace).toContain("选区折叠");
    expect(designSystem).toContain("选区折叠");

    expect(designSystem).toContain("预览不进入 IPC");
    expect(adaptiveWorkspace).toContain("候选预览只保存在 renderer 内存");
    expect(roadmap).toContain("仅在用户发送下一条消息时");
  });

  it("keeps the context menu clipboard-only and preserves document-level AI entry points", () => {
    const designSystem = readRepoFile("docs/design-system.md");
    const adaptiveWorkspace = readRepoFile("docs/adaptive-workspace.md");
    const roadmap = readRepoFile("ROADMAP.md");

    for (const document of [designSystem, adaptiveWorkspace, roadmap]) {
      expect(document).toContain("剪切、复制、粘贴、全选");
      expect(document).toContain("锁定/只读");
      expect(document).toMatch(/AI(?: ·)? 选区/u);
    }

    expect(designSystem).toContain("文档级命令");
    expect(roadmap).toContain("文档级 AI 命令仍由 `/` 菜单与 Agent 入口承载");
    expect(adaptiveWorkspace).toContain("任何 AI 选区改写、翻译、检查");
  });

  it("keeps selection previews out of the explicit IPC reference contract", () => {
    const ipcReference = readRepoFile("docs/ipc-api-reference.md");

    expect(ipcReference).toContain(
      "编辑器选区候选是 renderer 内存中的临时 UI 状态",
    );
    expect(ipcReference).toContain("assistant_run_start");
    expect(ipcReference).toContain("显式 `ContextReference`");
    expect(ipcReference).toContain(
      "选区预览文字不得进入 IPC、持久化事件、日志或会话",
    );
    expect(ipcReference).toContain(
      "classified 文档不走 normal-domain 选区引用",
    );
  });

  it("exposes only clipboard actions from the editor context menu", () => {
    const selected = filterEditorActions("context_menu", "editor", {
      hasNote: true,
      hasSelection: true,
      isLocked: false,
      streaming: false,
      aiDomain: "normal",
    });
    expect(selected.map((action) => action.id)).toEqual([
      "cut",
      "copy",
      "paste",
      "select-all",
    ]);

    const locked = filterEditorActions("context_menu", "editor", {
      hasNote: true,
      hasSelection: true,
      isLocked: true,
      streaming: false,
      aiDomain: "normal",
    });
    expect(locked.map((action) => action.id)).toEqual(["copy", "select-all"]);
  });
});
