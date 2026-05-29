import { describe, expect, it } from "vitest";

import {
  buildCommandPaletteItems,
  filterCommandPaletteItems,
  groupCommandPaletteItems,
} from "@/lib/command-palette";

describe("command palette", () => {
  it("includes recycle bin when vault is open", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const recycle = items.find((i) => i.id === "recycle-bin");
    expect(recycle?.disabled).toBe(false);
    expect(recycle?.shortcut).toContain("U");
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
    const groups = groupCommandPaletteItems(items);
    expect(groups[0]?.group).toBe("通用");
    expect(groups.some((g) => g.group === "AI")).toBe(true);
  });
});
