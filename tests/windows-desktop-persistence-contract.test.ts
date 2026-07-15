import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Windows 桌面 Markdown 持久化 E2E 入口", () => {
  const runnerPath = "scripts/run-windows-persistence-e2e.mjs";

  it("提供独立的 Windows 桌面执行入口，而不是复用 jsdom acceptance", () => {
    const pkg = JSON.parse(read("package.json")) as {
      scripts: Record<string, string>;
    };

    expect(pkg.scripts["test:desktop:windows"]).toBe(
      "node scripts/run-windows-persistence-e2e.mjs",
    );
    expect(read("vitest.e2e.config.ts")).toContain('environment: "jsdom"');
    expect(existsSync(runnerPath)).toBe(true);
  });

  it("使用 Tauri WebDriver 启动真实 exe、关闭并重启，而非模拟 invoke", () => {
    const runner = read(runnerPath);

    expect(runner).toContain('process.platform !== "win32"');
    expect(runner).toContain("tauri-driver");
    expect(runner).toContain('"tauri:options"');
    expect(runner).toContain('browserName: "wry"');
    expect(runner).toContain('data-testid="rail-new-note-button"');
    expect(runner).toContain('data-testid="document-title"');
    expect(runner).toContain('data-testid="editor"');
    expect(runner).toContain('aria-label="关闭"');
    expect(runner).toContain("restartApplication");
    expect(runner).toMatch(
      /invokeTauri[\s\S]*executeAsync[\s\S]*const done = arguments[\s\S]*__TAURI_INTERNALS__/,
    );
    expect(runner).not.toContain("vitest");
    expect(runner).not.toContain("vitest run");
  });

  it("在重启后直接读取临时 vault 的 UTF-8 Markdown 并作完整内容断言", () => {
    const runner = read(runnerPath);

    expect(runner).toContain("mkdtempSync");
    expect(runner).toContain("readFileSync");
    expect(runner).toContain("utf8");
    expect(runner).toContain("assertPersistedMarkdown");
    expect(runner).toContain("EXPECTED_TITLE");
    expect(runner).toContain("EXPECTED_BODY");
    expect(runner).toContain("rmSync");
  });

  it("在新 surface 实际挂载的 staging 阶段保存，并在同一 surface visible 后追加正文", () => {
    const runner = read(runnerPath);
    const workspace = read("src/components/layout/AppEditorWorkspace.tsx");

    expect(runner).toContain("REMOUNT_BODY_LINE");
    expect(runner).toContain("waitForMountedRemountStaging");
    expect(runner).toContain("waitForRemountVisible");
    expect(runner).toContain("REMOUNT_POLL_INTERVAL_MS");
    expect(runner).toContain("data-editor-active-surface-path");
    expect(runner).toContain("data-editor-active-surface-phase");
    expect(runner).toContain("data-editor-surface-identity");
    expect(runner).toContain("KEY.CONTROL}${KEY.END}");
    expect(runner).not.toContain("waitForRemountIdentity");
    expect(runner).not.toContain("click(sessionId, remountEditor)");
    expect(workspace).toContain('data-testid="editor-surface-stack"');
    expect(workspace).toContain(
      'data-editor-active-surface-path={activeSurfaceRecord?.snapshot.path ?? ""}',
    );
    expect(workspace).toContain(
      "data-editor-active-surface-phase={activeSurfacePhase}",
    );
    expect(runner).toMatch(
      /waitForMountedRemountStaging[\s\S]*pressSave\(sessionId\)[\s\S]*waitForRemountVisible[\s\S]*sendKeys\(sessionId, remountEditor, `\$\{KEY\.CONTROL\}\$\{KEY\.END\}`\)[\s\S]*sendKeys\(sessionId, remountEditor, REMOUNT_BODY_LINE\)[\s\S]*pressSave\(sessionId\)[\s\S]*aria-label="关闭"/,
    );
  });

  it("第二次真实启动后经应用 UI 打开重命名笔记并断言标题和全文", () => {
    const runner = read(runnerPath);
    const welcome = read("src/components/layout/WelcomeEmpty.tsx");

    expect(welcome).toContain('data-testid="home-recent-note"');
    expect(runner).toContain("openPersistedNoteInApplication");
    expect(runner).toContain('data-testid="home-recent-note"');
    expect(runner).toContain("assertOpenedNote");
  });

  it("将真实 Windows E2E 设为发布包构建后的硬门禁", () => {
    const workflow = read(".github/workflows/package-desktop.yml");

    expect(workflow).toContain("Install Tauri WebDriver tools");
    expect(workflow).toContain(
      "cargo install tauri-driver --version 2.0.6 --locked",
    );
    expect(workflow).toContain('Test-Path "$PWD/msedgedriver.exe"');
    expect(workflow).toContain("Run Windows Markdown persistence desktop E2E");
    expect(workflow).toContain("npm run test:desktop:windows");
    expect(workflow).toMatch(
      /Package Windows NSIS installer[\s\S]*Run Windows Markdown persistence desktop E2E/,
    );
  });

  it("将 Windows E2E 接入 PR CI，并固定测试工具版本", () => {
    const ci = read(".github/workflows/ci.yml");
    const release = read(".github/workflows/package-desktop.yml");

    expect(ci).toContain("Windows Markdown persistence desktop E2E");
    expect(ci).toMatch(/on:\n\s+pull_request:/);
    expect(ci).toContain("runs-on: windows-2022");
    expect(ci).toContain("npm run tauri -- build --no-bundle");
    expect(ci).toContain("npm run test:desktop:windows");
    expect(release).toContain("tauri-driver --version 2.0.6 --locked");
    expect(release).toContain("--rev 8c4b34f51b45f5cf08013366d703de464ab871d1");
  });

  it("准确说明 PR 合并门禁由仓库外的分支保护规则配置", () => {
    const acceptance = read(
      "docs/testing/document-persistence-embedding-acceptance.md",
    );

    expect(acceptance).toContain("分支保护");
    expect(acceptance).toContain("仓库外");
    expect(acceptance).not.toContain("PR CI 与发布打包 CI 的 Windows 硬门禁");
  });
});
