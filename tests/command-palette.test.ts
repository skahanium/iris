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
    expect(formatCommandPaletteItemShortcut(recycle!)).toContain("U");
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

  it("hides command-palette self-entry from the list", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const filtered = filterCommandPaletteItems(items, "命令面板");
    expect(filtered.some((i) => i.id === "command-palette")).toBe(false);
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
    expect(groups[0]?.group).toBe("导航");
    expect(groups.some((g) => g.group === "AI")).toBe(true);
    expect(groups.some((g) => g.group === "视图")).toBe(true);
    expect(groups.some((g) => g.group === "通用")).toBe(false);
    expect(groups.some((g) => g.group === "编辑器")).toBe(false);
    expect(groups.some((g) => g.group === "库")).toBe(false);
  });

  it("sorts by usage within groups without reordering groups", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: true,
    });
    const visible = items.filter((i) => !i.hiddenInPalette);
    const sorted = sortCommandPaletteItems(visible);
    const groups = groupCommandPaletteItems(sorted);
    expect(groups.map((g) => g.group)).toEqual(["导航", "笔记", "视图", "AI"]);
  });

  it("includes skills management in AI group", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const skills = items.find((i) => i.id === "skills");
    expect(skills?.group).toBe("AI");
    expect(skills?.action).toEqual({ type: "openOverlay", overlay: "skills" });
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
});
