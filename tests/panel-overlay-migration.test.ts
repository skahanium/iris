import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const PANEL_SPECS = [
  {
    path: "src/components/file/SearchPanel.tsx",
    size: "command",
  },
  {
    path: "src/components/file/FileSheet.tsx",
    size: "command",
  },
  {
    path: "src/components/settings/SettingsPanel.tsx",
    size: "command",
  },
  {
    path: "src/components/file/BacklinksPanel.tsx",
    size: "command",
  },
  {
    path: "src/components/tag/TagView.tsx",
    size: "command",
  },
  {
    path: "src/components/version/VersionTimeline.tsx",
    size: "wide",
  },
  {
    path: "src/components/graph/GraphView.tsx",
    size: "graph",
  },
  {
    path: "src/components/file/QuickOpen.tsx",
    size: "compact",
  },
] as const;

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("panel overlay migration", () => {
  it("uses IrisOverlay or compact Dialog sizing instead of SidePanel", () => {
    for (const spec of PANEL_SPECS) {
      const source = read(spec.path);

      expect(source, spec.path).not.toContain("@/components/ui/side-panel");
      expect(source, spec.path).not.toContain("<SidePanel");

      if (spec.path.endsWith("QuickOpen.tsx")) {
        expect(source, spec.path).toContain(`size="${spec.size}"`);
      } else {
        expect(source, spec.path).toContain("@/components/ui/iris-overlay");
        expect(source, spec.path).toContain(`size="${spec.size}"`);
      }
    }
  });

  it("removes aiPanelOpen plumbing from migrated panels and App overlay wiring", () => {
    for (const spec of PANEL_SPECS) {
      if (spec.path.endsWith("QuickOpen.tsx")) continue;
      expect(read(spec.path), spec.path).not.toContain("aiPanelOpen");
    }

    const appSource = read("src/App.tsx");
    for (const componentName of [
      "FileSheet",
      "SearchPanel",
      "SettingsPanel",
      "BacklinksPanel",
      "TagView",
      "VersionTimeline",
    ]) {
      const match = appSource.match(
        new RegExp(`<${componentName}[\\s\\S]*?\\n\\s*/>`),
      );
      expect(match?.[0] ?? "", componentName).not.toContain("aiPanelOpen");
    }
  });
});
