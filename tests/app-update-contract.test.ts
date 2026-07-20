import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("app update contract", () => {
  it("keeps the updater execution boundary in Rust IPC", () => {
    const commands = read("src-tauri/src/commands/mod.rs");
    const lib = read("src-tauri/src/lib.rs");
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/ipc.ts");
    const events = read("src/lib/ipc-events.ts");

    expect(commands).toContain("pub mod app_update");
    for (const command of [
      "app_update_check_cmd",
      "app_update_download_cmd",
      "app_update_preflight_cmd",
      "app_update_install_cmd",
    ]) {
      expect(lib).toContain(`commands::app_update::${command}`);
      expect(ipc).toContain(`"${command}"`);
    }

    for (const exported of [
      "AppUpdateStatus",
      "AppUpdateInfo",
      "AppUpdatePreflightResult",
      "AppUpdateProgressEvent",
    ]) {
      expect(types).toMatch(
        new RegExp(`export (type|interface) ${exported}\\b`),
      );
    }

    expect(events).toContain('APP_UPDATE_STATUS: "app-update:status"');
    expect(events).toContain('APP_UPDATE_PROGRESS: "app-update:progress"');
    expect(ipc).toContain("IPC_EVENTS.APP_UPDATE_STATUS");
    expect(ipc).toContain("IPC_EVENTS.APP_UPDATE_PROGRESS");
    expect(ipc).not.toContain("github.com/skahanium/iris/releases/download");
    expect(ipc).not.toContain("latest.json");
  });

  it("separates check, download, preflight, and install in the backend", () => {
    const updater = read("src-tauri/src/commands/app_update.rs");

    expect(updater).toContain("pub async fn app_update_check_cmd");
    expect(updater).toContain("pub async fn app_update_download_cmd");
    expect(updater).toContain("pub fn app_update_preflight_cmd");
    expect(updater).toContain("pub fn app_update_install_cmd");
    expect(updater).toContain("RANGE");
    expect(updater).toContain(".install(");
    expect(updater).not.toContain("download_and_install");
    expect(updater).toContain("PendingAppUpdate");
    expect(updater).toContain("verified_package");
    expect(updater).toContain("preflight_passed");
  });

  it("gates installation on local state inheritance checks", () => {
    const updater = read("src-tauri/src/commands/app_update.rs");

    for (const required of [
      "vault_path",
      "_migrations",
      "settings",
      "auto_version_enabled",
      "web_search_enabled",
      "llm_routing",
      "prompt_profile",
      "credential.configured.",
      "master.key",
      "versions",
      "recycle_bin",
      "sessions",
      "session_messages",
      "session_evidence",
      ".classified",
      ".iris",
      "cas",
    ]) {
      expect(updater).toContain(required);
    }

    expect(updater).not.toContain("decrypt_cef");
    expect(updater).not.toContain("get_runtime_secret");
  });

  it("surfaces update state in the status bar and About panel", () => {
    const statusBar = read("src/components/layout/StatusBar.tsx");
    const slot = read("src/components/layout/AppStatusBarSlot.tsx");
    const about = read("src/components/settings/ManagementCenterPanel.tsx");
    const app = read("src/App.impl.tsx");
    const hook = read("src/hooks/useAppUpdate.ts");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const types = read("src/types/ipc.ts");

    expect(statusBar).toContain('data-testid="status-bar-update-available"');
    expect(slot).toContain("appUpdate");
    expect(app).toContain("useAppUpdateController");
    expect(hook).toContain("beforeInstall");
    expect(hook).toContain("await beforeInstall()");
    expect(hook).not.toContain("hasUnsaved");
    expect(hook).toContain("handleActionError");
    expect(hook).toContain("busy: false");
    expect(overlays).toContain("hasDirtyDocuments");
    expect(about).toContain("下载更新");
    expect(about).toContain("安装并重启");
    expect(about).toContain("无法检查更新");
    expect(about).toContain("更新包验证失败");
    expect(about).toContain("当前平台暂不支持应用内更新");
    expect(about).toContain("isWindowsDesktopChrome");
    expect(about).toContain("会关闭 Iris");
    expect(about).not.toContain("发布时间");
    expect(about).toContain("从已缓存内容继续下载");
    expect(types).not.toContain("pubDate");
  });

  it("keeps update failures retryable and hides low-level transport errors", () => {
    const hook = read("src/hooks/useAppUpdate.ts");
    const updater = read("src-tauri/src/commands/app_update.rs");
    const about = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(hook).toContain("catch");
    expect(hook).toContain("无法连接更新服务器，请检查网络后重试");
    expect(hook).toContain(
      "更新安装失败，请重试或前往 GitHub Release 手动安装",
    );
    expect(hook).not.toContain("error sending request");
    expect(updater).toContain("APP_UPDATE_CHECK_TIMEOUT");
    expect(updater).toContain(".timeout(APP_UPDATE_CHECK_TIMEOUT)");
    expect(updater).toContain("sanitize_check_error_message");
    expect(updater).toContain("当前发布暂不支持应用内更新");
    expect(updater).not.toContain('format!("无法检查更新：{err}")');
    expect(about).not.toContain("<p>Windows 上安装会关闭 Iris。</p>");
    expect(about).toContain('appUpdate.status === "ready_to_install"');
  });

  it("configures Tauri updater and GitHub Release updater assets", () => {
    const tauriConfig = read("src-tauri/tauri.conf.json");
    const cargoToml = read("src-tauri/Cargo.toml");
    const workflow = read(".github/workflows/package-desktop.yml");

    expect(cargoToml).toContain("tauri-plugin-updater");
    expect(tauriConfig).toContain('"createUpdaterArtifacts": true');
    expect(tauriConfig).toContain('"updater"');
    expect(tauriConfig).toContain('"pubkey"');
    expect(tauriConfig).not.toContain(
      "REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY_BEFORE_STABLE_RELEASE",
    );
    expect(tauriConfig).toContain(
      "https://github.com/skahanium/iris/releases/latest/download/latest.json",
    );
    expect(tauriConfig).toContain('"installMode": "passive"');

    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY");
    expect(workflow).toContain("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    expect(workflow).toContain(".app.tar.gz");
    expect(workflow).toContain(".app.tar.gz.sig");
    expect(workflow).toContain("*setup.exe.sig");
    expect(workflow).toContain("latest.json");
    expect(workflow).toContain("scripts/build-updater-manifest.mjs");
    expect(workflow).toContain("--draft");
    expect(workflow).toContain("--clobber");
    expect(existsSync("scripts/build-updater-manifest.mjs")).toBe(true);
    expect(existsSync("scripts/verify-updater-release.mjs")).toBe(true);
    expect(existsSync("scripts/verify-desktop-package.mjs")).toBe(true);
  });

  it("enables reqwest system-proxy so Clash/V2Ray system proxy accelerates updates", () => {
    const cargoToml = read("src-tauri/Cargo.toml");
    const proxyPolicy = read("src-tauri/src/network/proxy_policy.rs");
    const httpsClient = read("src-tauri/src/network/cert_pinning.rs");
    const reqwestLine = cargoToml
      .split("\n")
      .find((line) => line.trimStart().startsWith("reqwest "));

    expect(reqwestLine).toBeDefined();
    expect(reqwestLine).toContain("system-proxy");
    expect(reqwestLine).toContain("socks");
    expect(reqwestLine).toContain("default-features = false");
    expect(proxyPolicy).toContain("apply_proxy_policy");
    expect(proxyPolicy).toContain(".no_proxy()");
    // Base builders must not hard-disable proxy; only the policy helper may.
    expect(httpsClient).not.toMatch(
      /base_https_client_builder[\s\S]*?\.no_proxy\(\)/,
    );
  });

  it("exposes a follow_system_proxy setting with management-center toggle", () => {
    const cargoPolicy = read("src-tauri/src/security/ipc_policy.rs");
    const ipc = read("src/lib/ipc.ts");
    const hook = read("src/hooks/useFollowSystemProxy.ts");
    const panel = read("src/components/settings/ManagementCenterPanel.tsx");
    const appUpdate = read("src-tauri/src/commands/app_update.rs");

    expect(cargoPolicy).toContain('"follow_system_proxy"');
    expect(ipc).toContain("follow_system_proxy: boolean");
    expect(hook).toContain("DEFAULT_FOLLOW_SYSTEM_PROXY = true");
    expect(hook).toContain('settingsSet("follow_system_proxy"');
    expect(panel).toContain('data-testid="follow-system-proxy-switch"');
    expect(panel).toContain('data-testid="follow-system-proxy-status"');
    expect(panel).toContain("使用系统代理");
    expect(panel).toContain("proxyStatusLabel");
    expect(panel).not.toContain("Clash、V2Ray");
    expect(ipc).toContain("network_proxy_status");
    expect(hook).toContain("proxyStatusLabel");
    expect(appUpdate).toContain("apply_proxy_policy");
    expect(appUpdate).toContain("follow_system_proxy");
  });
});
