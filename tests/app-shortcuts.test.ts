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
  });

  it("keeps former command palette entries reachable only as management actions", () => {
    const shortcuts = buildAppShortcutItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const byId = new Map(shortcuts.map((item) => [item.id, item]));

    for (const id of [
      "file-sheet",
      "recycle-bin",
      "knowledge-relations",
      "graph",
      "toggle-outline",
      "skills",
      "toggle-web-search",
      "rescan-vault",
    ]) {
      expect(byId.get(id)?.chord, id).toBeUndefined();
    }
  });
});
