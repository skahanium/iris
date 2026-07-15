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

  it("观察重命名 remount 的 staging 保存，并在新 editor 可交互后输入唯一正文立即保存关闭", () => {
    const runner = read(runnerPath);

    expect(runner).toContain("REMOUNT_BODY_LINE");
    expect(runner).toContain("waitForRemountStaging");
    expect(runner).toContain("waitForRemountVisible");
    expect(runner).toContain('data-editor-visibility="staging"');
    expect(runner).toContain('END: "\\uE010"');
    expect(runner).toMatch(
      /waitForRemountStaging[\s\S]*pressSave\(sessionId\)[\s\S]*waitForRemountVisible[\s\S]*sendKeys\(sessionId, remountEditor, REMOUNT_BODY_LINE\)[\s\S]*pressSave\(sessionId\)[\s\S]*aria-label="关闭"/,
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

  it("将 Windows E2E 设为当前提交的 PR/push CI 硬门禁，并固定测试工具版本", () => {
    const ci = read(".github/workflows/ci.yml");
    const release = read(".github/workflows/package-desktop.yml");

    expect(ci).toContain("Windows Markdown persistence desktop E2E");
    expect(ci).toContain("runs-on: windows-2022");
    expect(ci).toContain("npm run tauri -- build --no-bundle");
    expect(ci).toContain("npm run test:desktop:windows");
    expect(release).toContain("tauri-driver --version 2.0.6 --locked");
    expect(release).toContain("--rev 8c4b34f51b45f5cf08013366d703de464ab871d1");
  });
});
