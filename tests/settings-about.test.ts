import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("SettingsPanel about and legal notice", () => {
  it("shows copyright and AGPL license information without a product watermark", () => {
    const source = read("src/components/settings/SettingsPanel.tsx");

    expect(source).toContain("关于 Iris");
    expect(source).toContain("Iris");
    expect(source).toContain("版本 1.0.0");
    expect(source).toContain("Copyright (C) 2026 Iris Contributors");
    expect(source).toContain("GNU Affero General Public License v3.0");
    expect(source).not.toContain("开发者水印");
  });
});
