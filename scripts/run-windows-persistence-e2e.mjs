#!/usr/bin/env node
/**
 * Windows-only desktop acceptance for the Markdown-first persistence path.
 *
 * This is deliberately a small, direct W3C WebDriver client rather than a
 * browser/jsdom test: tauri-driver launches the built Iris executable, then
 * Edge WebDriver sends real input to the Tauri WebView. The test fixture uses
 * an isolated IRIS_HOME and validates the resulting Markdown on disk after a
 * real window close and a second application launch.
 *
 * Prerequisites (test tools only; not product dependencies):
 *   - tauri-driver on PATH (or IRIS_TAURI_DRIVER)
 *   - a matching msedgedriver.exe on PATH (or IRIS_EDGE_DRIVER)
 *   - a built Iris Windows executable (or IRIS_DESKTOP_E2E_APP)
 */
import { spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const WEBDRIVER_URL = "http://127.0.0.1:4444";
const SESSION_TIMEOUT_MS = 45_000;
const POLL_INTERVAL_MS = 125;
const KEY = {
  CONTROL: "\uE009",
  END: "\uE010",
  ENTER: "\uE007",
};

const FIRST_BODY_LINE = "IRIS_E2E_FIRST_LINE";
const REMOUNT_BODY_LINE = "IRIS_E2E_REMOUNT_LINE";
const EXPECTED_TITLE = "IRIS_E2E_RENAMED_TITLE";
const EXPECTED_FILE_NAME = `${EXPECTED_TITLE}.md`;
const ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf";

function fail(code) {
  throw new Error(code);
}

function debugStep(step) {
  if (process.env.IRIS_DESKTOP_E2E_DEBUG === "1") {
    process.stderr.write(`[desktop-e2e-debug] step=${step}\n`);
  }
}

function safeFailureCode(error) {
  if (error instanceof Error && /^[a-z0-9_]+$/.test(error.message)) {
    return error.message;
  }
  return "desktop_e2e_unexpected_error";
}

function assertWindows() {
  if (process.platform !== "win32") {
    fail("windows_desktop_e2e_requires_windows");
  }
}

function defaultApplicationPath() {
  return path.join(root, ".iris-dev", "target", "release", "iris.exe");
}

function applicationPath() {
  const candidate =
    process.env.IRIS_DESKTOP_E2E_APP || defaultApplicationPath();
  if (!existsSync(candidate)) fail("desktop_e2e_application_not_found");
  return path.resolve(candidate);
}

function buildFixtureEnvironment(fixtureRoot) {
  const stateRoot = path.join(fixtureRoot, "state");
  const edgeDriver = process.env.IRIS_EDGE_DRIVER;
  if (edgeDriver && !existsSync(edgeDriver))
    fail("desktop_e2e_edge_driver_not_found");

  const currentPath = process.env.PATH || "";
  const edgePath = edgeDriver ? path.dirname(edgeDriver) : "";
  return {
    ...process.env,
    IRIS_HOME: stateRoot,
    IRIS_DATA_DIR: path.join(stateRoot, "app-data"),
    IRIS_CACHE_DIR: path.join(stateRoot, "cache"),
    IRIS_TEMP_DIR: path.join(stateRoot, "tmp"),
    IRIS_GLOBAL_SKILLS_DIR: path.join(stateRoot, "skills"),
    PATH: edgePath ? `${edgePath}${path.delimiter}${currentPath}` : currentPath,
  };
}

function startTauriDriver(env) {
  const executable = process.env.IRIS_TAURI_DRIVER || "tauri-driver";
  const child = spawn(executable, [], {
    cwd: root,
    env,
    stdio: ["ignore", "inherit", "inherit"],
    windowsHide: true,
  });
  child.once("error", () => {
    // The readiness poll reports a stable result code without leaking paths.
  });
  return child;
}

async function webdriverRequest(method, pathname, body) {
  const response = await fetch(`${WEBDRIVER_URL}${pathname}`, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok || !payload || payload.value?.error) {
    if (process.env.IRIS_DESKTOP_E2E_DEBUG === "1") {
      process.stderr.write(
        `[desktop-e2e-debug] ${method} ${pathname} status=${response.status}\n`,
      );
    }
    fail(`webdriver_${method.toLowerCase()}_failed`);
  }
  return payload.value;
}

async function waitUntil(predicate, code, pollIntervalMs = POLL_INTERVAL_MS) {
  const deadline = Date.now() + SESSION_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch {
      // During first launch/reload/close WebDriver legitimately reports no
      // current window. The timeout produces the stable test result code.
    }
    await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
  }
  fail(code);
}

async function waitForDriver() {
  await waitUntil(async () => {
    const response = await fetch(`${WEBDRIVER_URL}/status`);
    return response.ok;
  }, "tauri_driver_not_ready");
}

function sessionIdFrom(value) {
  const id = value?.sessionId;
  if (typeof id !== "string" || !id) fail("webdriver_session_id_missing");
  return id;
}

async function createSession(appPath) {
  const value = await webdriverRequest("POST", "/session", {
    capabilities: {
      alwaysMatch: {
        browserName: "wry",
        "tauri:options": { application: appPath },
      },
    },
  });
  return sessionIdFrom(value);
}

async function deleteSession(sessionId) {
  try {
    await webdriverRequest("DELETE", `/session/${sessionId}`);
  } catch {
    // A genuine window close ends the first session before this cleanup call.
  }
}

function elementId(value) {
  const id = value?.[ELEMENT_KEY] ?? value?.ELEMENT;
  if (typeof id !== "string" || !id) fail("webdriver_element_id_missing");
  return id;
}

async function findElement(sessionId, selector) {
  const value = await webdriverRequest(
    "POST",
    `/session/${sessionId}/element`,
    {
      using: "css selector",
      value: selector,
    },
  );
  return elementId(value);
}

async function waitForElement(sessionId, selector) {
  return waitUntil(
    () => findElement(sessionId, selector),
    "desktop_element_not_found",
  );
}

async function click(sessionId, element) {
  await webdriverRequest(
    "POST",
    `/session/${sessionId}/element/${element}/click`,
    {},
  );
}

async function sendKeys(sessionId, element, text) {
  await webdriverRequest(
    "POST",
    `/session/${sessionId}/element/${element}/value`,
    {
      text,
      value: Array.from(text),
    },
  );
}

async function elementText(sessionId, element) {
  return webdriverRequest(
    "GET",
    `/session/${sessionId}/element/${element}/text`,
  );
}

async function executeAsync(sessionId, script, args = []) {
  return webdriverRequest("POST", `/session/${sessionId}/execute/async`, {
    script,
    args,
  });
}

async function executeSync(sessionId, script, args = []) {
  return webdriverRequest("POST", `/session/${sessionId}/execute/sync`, {
    script,
    args,
  });
}

async function invokeTauri(sessionId, command, payload) {
  const result = await executeAsync(
    sessionId,
    `
      const done = arguments[arguments.length - 1];
      const [command, payload] = arguments;
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") {
        done({ ok: false });
        return;
      }
      Promise.resolve(invoke(command, payload))
        .then(() => done({ ok: true }))
        .catch(() => done({ ok: false }));
    `,
    [command, payload],
  );
  if (result?.ok !== true) fail("tauri_fixture_command_failed");
}

async function debugTauriCatalogState(sessionId) {
  if (process.env.IRIS_DESKTOP_E2E_DEBUG !== "1") return;
  const result = await executeAsync(
    sessionId,
    `
      const done = arguments[arguments.length - 1];
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== 'function') {
        done({ catalogCount: null, vaultReady: false });
        return;
      }
      Promise.allSettled([invoke('vault_get'), invoke('file_list', { limit: 5, offset: 0 })])
        .then(async ([vault, files]) => {
          const firstPath = files.status === 'fulfilled' && Array.isArray(files.value)
            ? files.value[0]?.path
            : null;
          const read = typeof firstPath === 'string'
            ? await Promise.resolve(invoke('file_read', { path: firstPath, allowClassified: false }))
                .then(() => true)
                .catch(() => false)
            : false;
          done({
            catalogCount: files.status === 'fulfilled' && Array.isArray(files.value)
              ? files.value.length
              : null,
            fileReadReady: read,
            vaultReady: vault.status === 'fulfilled' && typeof vault.value === 'string' && vault.value.length > 0,
          });
        });
    `,
  );
  process.stderr.write(
    `[desktop-e2e-debug] restart-catalog=${JSON.stringify(result)}\n`,
  );
}

async function reloadWebview(sessionId) {
  await webdriverRequest("POST", `/session/${sessionId}/execute/sync`, {
    script: "window.location.reload(); return true;",
    args: [],
  });
}

async function pressSave(sessionId) {
  await webdriverRequest("POST", `/session/${sessionId}/actions`, {
    actions: [
      {
        type: "key",
        id: "iris-persistence-e2e-keyboard",
        actions: [
          { type: "keyDown", value: KEY.CONTROL },
          { type: "keyDown", value: "s" },
          { type: "keyUp", value: "s" },
          { type: "keyUp", value: KEY.CONTROL },
        ],
      },
    ],
  });
}

async function focusDocumentTitle(sessionId) {
  const focused = await executeSync(
    sessionId,
    `
      const input = document.querySelector('[data-testid="document-title"]');
      if (!(input instanceof HTMLTextAreaElement)) return false;
      input.focus();
      return document.activeElement === input;
    `,
    [],
  );
  if (focused !== true) fail("title_input_not_focused");
}

async function typeFocusedText(sessionId, text) {
  await webdriverRequest("POST", `/session/${sessionId}/actions`, {
    actions: [
      {
        type: "key",
        id: "iris-persistence-e2e-typing",
        actions: Array.from(text).flatMap((value) => [
          { type: "keyDown", value },
          { type: "keyUp", value },
        ]),
      },
    ],
  });
}

async function sessionHasClosed(sessionId) {
  try {
    await webdriverRequest("GET", `/session/${sessionId}/title`);
    return false;
  } catch {
    return true;
  }
}

async function restartApplication(sessionId, appPath) {
  await waitUntil(
    () => sessionHasClosed(sessionId),
    "window_close_did_not_exit",
  );
  await deleteSession(sessionId);
  return createSession(appPath);
}

function markdownFile(vaultPath) {
  const notes = readdirSync(vaultPath, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => entry.name);
  if (notes.length !== 1) fail("unexpected_markdown_file_count");
  return path.join(vaultPath, notes[0]);
}

function expectedMarkdownFile(vaultPath) {
  const file = path.join(vaultPath, EXPECTED_FILE_NAME);
  if (!existsSync(file)) fail("renamed_markdown_file_missing");
  return file;
}

function readVaultMarkdown(vaultPath) {
  return readFileSync(markdownFile(vaultPath), "utf8");
}

async function waitForPersistedBody(vaultPath) {
  await waitUntil(() => {
    try {
      const markdown = readVaultMarkdown(vaultPath);
      return (
        markdown.includes(FIRST_BODY_LINE) &&
        markdown.includes(REMOUNT_BODY_LINE)
      );
    } catch {
      return false;
    }
  }, "persisted_body_missing");
}

function assertPersistedMarkdown(vaultPath) {
  const markdown = readFileSync(expectedMarkdownFile(vaultPath), "utf8");
  if (!markdown.includes(FIRST_BODY_LINE)) fail("markdown_first_line_missing");
  if (!markdown.includes(REMOUNT_BODY_LINE)) {
    fail("markdown_remount_line_missing");
  }
  if (/^---\r?\ntitle:/m.test(markdown)) fail("legacy_markdown_title_present");
}

function normalizedEditorText(value) {
  return String(value).replaceAll("\r\n", "\n").trim();
}

async function openPersistedNoteInApplication(sessionId) {
  await waitForElement(sessionId, '[data-testid="home-workbench"]');
  const recentNote = await waitForElement(
    sessionId,
    '[data-testid="home-recent-note"]',
  );
  await click(sessionId, recentNote);
}

async function assertOpenedNote(sessionId) {
  debugStep("reopened-title");
  const title = await waitForElement(
    sessionId,
    '[data-testid="document-title"]',
  );
  const titleValue = await executeSync(
    sessionId,
    "return arguments[0].value;",
    [{ [ELEMENT_KEY]: title }],
  );
  if (titleValue !== EXPECTED_TITLE) fail("reopened_title_mismatch");
  debugStep("reopened-editor");
  const editor = await waitForElement(
    sessionId,
    '[data-editor-visibility="visible"] [contenteditable="true"]',
  );
  const text = normalizedEditorText(await elementText(sessionId, editor));
  if (!text.includes(FIRST_BODY_LINE) || !text.includes(REMOUNT_BODY_LINE)) {
    fail("reopened_editor_body_mismatch");
  }
}

async function runScenario(sessionId, vaultPath) {
  debugStep("wait-home");
  await waitForElement(sessionId, '[data-testid="home-workbench"]');

  debugStep("new-note");
  const newNote = await waitForElement(
    sessionId,
    '[data-testid="rail-new-note-button"]',
  );
  await click(sessionId, newNote);

  debugStep("find-title");
  await new Promise((resolve) => setTimeout(resolve, 1000));
  await waitForElement(sessionId, '[data-testid="document-title"]');
  debugStep("title-type");
  // Tauri WebDriver intermittently invalidates textarea `/value` element
  // references. Its global key actions remain stable, so focus the real field
  // and use the same native keyboard path as a user.
  await focusDocumentTitle(sessionId);
  await webdriverRequest("POST", `/session/${sessionId}/actions`, {
    actions: [
      {
        type: "key",
        id: "iris-persistence-e2e-title-select",
        actions: [
          { type: "keyDown", value: KEY.CONTROL },
          { type: "keyDown", value: "a" },
          { type: "keyUp", value: "a" },
          { type: "keyUp", value: KEY.CONTROL },
        ],
      },
    ],
  });
  await typeFocusedText(sessionId, EXPECTED_TITLE);
  await typeFocusedText(sessionId, KEY.ENTER);
  debugStep("wait-rename");
  await waitUntil(
    () => existsSync(path.join(vaultPath, EXPECTED_FILE_NAME)),
    "title_rename_not_persisted",
  );
  await waitUntil(
    () =>
      executeSync(
        sessionId,
        `
          return document
            .querySelector('[data-testid="editor-surface-stack"]')
            ?.getAttribute('data-editor-active-surface-path') === arguments[0];
        `,
        [EXPECTED_FILE_NAME],
      ),
    "renamed_surface_path_missing",
  );

  const editor = await waitForElement(
    sessionId,
    '[data-editor-visibility="visible"] [contenteditable="true"]',
  );
  await click(sessionId, editor);
  // Continue typing immediately after the rename commit. This catches a stale
  // old-path autosave and a remounted editor surface.
  await sendKeys(sessionId, editor, FIRST_BODY_LINE);
  await sendKeys(sessionId, editor, KEY.ENTER);
  await sendKeys(sessionId, editor, KEY.ENTER);
  await sendKeys(sessionId, editor, REMOUNT_BODY_LINE);
  await pressSave(sessionId);
  debugStep("wait-body-save");
  await waitForPersistedBody(vaultPath);

  debugStep("close-first-window");
  const close = await waitForElement(sessionId, '[aria-label="关闭"]');
  await click(sessionId, close);

  // Do not read the file until the second Tauri process is running: this
  // proves the persisted Markdown survives both close and startup boundaries.
}

async function main() {
  assertWindows();
  const appPath = applicationPath();
  const fixtureRoot = mkdtempSync(
    path.join(os.tmpdir(), "iris-persistence-e2e-"),
  );
  const vaultPath = path.join(fixtureRoot, "vault");
  mkdirSync(vaultPath, { recursive: true });

  let driver;
  let sessionId;
  let passed = false;
  try {
    const env = buildFixtureEnvironment(fixtureRoot);
    driver = startTauriDriver(env);
    await waitForDriver();
    sessionId = await createSession(appPath);

    await waitUntil(
      () =>
        executeSync(
          sessionId,
          "return typeof window.__TAURI_INTERNALS__?.invoke === 'function';",
        ),
      "tauri_runtime_not_ready",
    );
    await invokeTauri(sessionId, "vault_set", { path: vaultPath });
    await reloadWebview(sessionId);

    await runScenario(sessionId, vaultPath);
    debugStep("restart");
    sessionId = await restartApplication(sessionId, appPath);
    await waitForElement(sessionId, '[data-testid="desktop-title-bar"]');
    assertPersistedMarkdown(vaultPath);
    await debugTauriCatalogState(sessionId);
    debugStep("open-recent");
    await openPersistedNoteInApplication(sessionId);
    debugStep("assert-reopened");
    await assertOpenedNote(sessionId);
    passed = true;
    process.stdout.write("[desktop-e2e] Windows Markdown persistence passed\n");
  } finally {
    if (sessionId) await deleteSession(sessionId);
    if (driver && !driver.killed) driver.kill();
    if (passed) rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

main().catch((error) => {
  // Do not include Markdown, title, vault path, or raw driver errors in logs.
  process.stderr.write(
    `[desktop-e2e] Windows Markdown persistence failed: ${safeFailureCode(error)}\n`,
  );
  process.exitCode = 1;
});
