import { describe, expect, it } from "vitest";

import { buildAppShortcutItems } from "@/lib/app-shortcuts";

describe("app shortcuts", () => {
  it("keeps core direct shortcuts without command palette or Cmd/Ctrl+K capture", () => {
    const shortcuts = buildAppShortcutItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const byId = new Map(shortcuts.map((item) => [item.id, item]));
    const serialized = JSON.stringify(shortcuts);

    expect(byId.has("command-palette")).toBe(false);
    expect(serialized).not.toContain("commandPalette");
    expect(serialized).not.toContain('"key":"K"');

    expect(byId.get("quick-open")?.chord).toEqual({
      key: "P",
      mod: true,
      requireVault: true,
    });
    expect(byId.get("search")?.chord).toEqual({
      key: "F",
      mod: true,
      shift: true,
      requireVault: true,
    });
    expect(byId.get("document-find")?.chord).toEqual({
      key: "F",
      mod: true,
      requireNote: true,
    });
    expect(byId.get("document-replace")?.chord).toEqual({
      key: "H",
      mod: true,
      requireNote: true,
    });
    expect(byId.get("save-note")?.chord).toEqual({
      key: "S",
      mod: true,
      requireNote: true,
    });
    expect(byId.get("version")?.chord).toEqual({
      key: "V",
      mod: true,
      shift: true,
      requireNote: true,
    });
    expect(byId.get("toggle-ai")?.chord).toEqual({
      key: "A",
      mod: true,
      shift: true,
    });
    expect(byId.get("management-center")?.chord).toEqual({
      key: ",",
      mod: true,
    });
    expect(byId.get("classified-panel")?.chord).toEqual({
      key: "L",
      mod: true,
      shift: true,
      requireVault: true,
    });
    expect(byId.get("file-sheet")?.chord).toEqual({
      key: "E",
      mod: true,
      shift: true,
      requireVault: true,
    });
    expect(byId.get("toggle-navigator")?.chord).toEqual({
      key: "\\",
      mod: true,
      requireVault: true,
    });
  });

  it("gates toggle-navigator to vaults and keeps file-sheet on full library management", () => {
    const withVault = new Map(
      buildAppShortcutItems({
        hasVault: true,
        hasActiveNote: true,
      }).map((item) => [item.id, item]),
    );
    const noVault = new Map(
      buildAppShortcutItems({
        hasVault: false,
        hasActiveNote: false,
      }).map((item) => [item.id, item]),
    );

    // 无 vault 时禁用；有 vault 时可用。
    expect(noVault.get("toggle-navigator")?.disabled).toBe(true);
    expect(withVault.get("toggle-navigator")?.disabled).toBe(false);
    expect(withVault.get("toggle-navigator")?.action).toEqual({
      type: "toggleNavigator",
    });
    // Ctrl/Cmd+Shift+E 仍为完整库管理，不与轻量导航快捷键冲突。
    expect(withVault.get("file-sheet")?.action).toEqual({
      type: "openManagementCenter",
      section: "notes",
      detail: "file-sheet",
    });
  });

  it("keeps former command palette entries reachable only as management actions", () => {
    const shortcuts = buildAppShortcutItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const byId = new Map(shortcuts.map((item) => [item.id, item]));

    for (const id of [
      "recycle-bin",
      "knowledge-relations",
      "graph",
      "toggle-outline",
      "toggle-zen",
      "skills",
      "toggle-web-search",
      "rescan-vault",
    ]) {
      expect(byId.get(id)?.chord, id).toBeUndefined();
    }
  });

  it("does not register a manual send-selection shortcut", () => {
    const shortcuts = buildAppShortcutItems({
      hasVault: true,
      hasActiveNote: true,
    });
    expect(shortcuts.some((item) => item.id === "send-selection-ai")).toBe(
      false,
    );
    expect(JSON.stringify(shortcuts)).not.toContain("sendSelectionToAi");
  });
});
