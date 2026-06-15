import { describe, expect, it } from "vitest";

import {
  buildCommandPaletteItems,
  filterCommandPaletteItems,
  formatCommandPaletteItemShortcut,
  groupCommandPaletteItems,
  sortCommandPaletteItems,
} from "@/lib/command-palette";

describe("command palette", () => {
  it("includes recycle bin when vault is open", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const recycle = items.find((i) => i.id === "recycle-bin");
    expect(recycle?.disabled).toBe(false);
    expect(formatCommandPaletteItemShortcut(recycle!)).toBeUndefined();
  });

  it("disables recycle bin without vault", () => {
    const items = buildCommandPaletteItems({
      hasVault: false,
      hasActiveNote: false,
    });
    expect(items.find((i) => i.id === "recycle-bin")?.disabled).toBe(true);
  });

  it("disables note-only commands without an active note", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const version = items.find((i) => i.id === "version");
    const quickOpen = items.find((i) => i.id === "quick-open");
    expect(version?.disabled).toBe(true);
    expect(quickOpen?.disabled).toBe(false);
  });

  it("filters by label and keyword", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const filtered = filterCommandPaletteItems(items, "图谱");
    expect(filtered.some((i) => i.id === "graph")).toBe(true);
    expect(filtered.some((i) => i.id === "settings")).toBe(false);
  });

  it("keeps build order when query is empty", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const filtered = filterCommandPaletteItems(items, "");
    const visibleIds = items.filter((i) => !i.hiddenInPalette).map((i) => i.id);
    expect(filtered.map((i) => i.id)).toEqual(visibleIds);
  });

  it("does not include the retired command-palette self-entry", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    expect(items.some((i) => i.id === "command-palette")).toBe(false);
    expect(JSON.stringify(items)).not.toContain(
      '"key":"P","mod":true,"shift":true',
    );
  });

  it("keeps list order when filtering including disabled items", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const filtered = filterCommandPaletteItems(items, "版本");
    const versionIndex = items.findIndex((i) => i.id === "version");
    const filteredVersionIndex = filtered.findIndex((i) => i.id === "version");
    expect(filteredVersionIndex).toBeGreaterThanOrEqual(0);
    if (filtered.length > 1) {
      expect(filtered[filteredVersionIndex]?.disabled).toBe(true);
    }
    expect(versionIndex).toBeGreaterThanOrEqual(0);
  });

  it("groups items by category in stable order", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const visible = items.filter((i) => !i.hiddenInPalette);
    const groups = groupCommandPaletteItems(visible);
    expect(groups[0]?.group).toBe("知识库");
    expect(groups.some((g) => g.group === "AI")).toBe(true);
    expect(groups.some((g) => g.group === "笔记")).toBe(true);
    expect(groups.some((g) => g.group === "系统")).toBe(true);
    expect(groups.some((g) => g.group === "导航")).toBe(false);
    expect(groups.some((g) => g.group === "视图")).toBe(false);
  });

  it("sorts by usage within groups without reordering groups", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const visible = items.filter((i) => !i.hiddenInPalette);
    const sorted = sortCommandPaletteItems(visible);
    const groups = groupCommandPaletteItems(sorted);
    expect(groups.map((g) => g.group)).toEqual([
      "知识库",
      "笔记",
      "系统",
      "AI",
    ]);
  });

  it("routes management commands into the management center", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const management = items.find((i) => i.id === "management-center");
    expect(management?.label).toBe("管理中心");
    expect(management?.chord).toEqual({ key: ",", mod: true });
    expect(management?.action).toEqual({
      type: "openManagementCenter",
      section: "overview",
    });

    const fileSheet = items.find((i) => i.id === "file-sheet");
    expect(fileSheet?.action).toEqual({
      type: "openManagementCenter",
      section: "notes",
      detail: "file-sheet",
    });

    const recycleBin = items.find((i) => i.id === "recycle-bin");
    expect(recycleBin?.action).toEqual({
      type: "openManagementCenter",
      section: "notes",
      detail: "recycle-bin",
    });

    const aiCenter = items.find((i) => i.id === "ai-system-center");
    expect(aiCenter?.action).toEqual({
      type: "openManagementCenter",
      section: "ai",
    });

    const skills = items.find((i) => i.id === "skills");
    expect(skills?.group).toBe("AI");
    expect(skills?.action).toEqual({
      type: "openManagementCenter",
      section: "ai",
    });
  });

  it("keeps only core global shortcuts and removes the app-level leader key", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const byId = new Map(items.map((item) => [item.id, item]));

    expect(byId.has("leader-cmd-k")).toBe(false);
    expect(JSON.stringify(items)).not.toContain("leader");
    expect(JSON.stringify(items)).not.toContain("afterLeader");

    expect(byId.get("file-sheet")?.chord).toEqual({
      key: "E",
      mod: true,
      shift: true,
      requireVault: true,
    });

    for (const id of [
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

    expect(byId.has("save-version")).toBe(false);

    expect(byId.get("version")?.chord).toEqual({
      key: "V",
      mod: true,
      shift: true,
      requireNote: true,
    });
    expect(byId.get("classified-panel")?.chord).toEqual({
      key: "L",
      mod: true,
      shift: true,
      requireVault: true,
    });
  });

  it("does not register slash writing commands in the palette", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    expect(items.some((i) => i.id.startsWith("slash-"))).toBe(false);
    const filtered = filterCommandPaletteItems(items, "AI 总结");
    expect(filtered.some((i) => i.id === "slash-summarize")).toBe(false);
  });

  it("keeps send-selection-ai for global shortcut path", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    expect(items.find((i) => i.id === "send-selection-ai")?.action).toEqual({
      type: "sendSelectionToAi",
    });
  });

  it("separates document find, document replace, and global search shortcuts", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    expect(items.find((i) => i.id === "document-find")?.chord).toEqual({
      key: "F",
      mod: true,
      requireNote: true,
    });
    expect(items.find((i) => i.id === "document-find")?.action).toEqual({
      type: "openFindReplace",
      mode: "find",
    });
    expect(items.find((i) => i.id === "document-replace")?.chord).toEqual({
      key: "H",
      mod: true,
      requireNote: true,
    });
    expect(items.find((i) => i.id === "document-replace")?.action).toEqual({
      type: "openFindReplace",
      mode: "replace",
    });
    expect(items.find((i) => i.id === "search")?.chord).toEqual({
      key: "F",
      mod: true,
      shift: true,
      requireVault: true,
    });
  });
});
