import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("ManagementCenterPanel system and legal notice", () => {
  it("merges system and about information into one management section", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(source).toContain('data-testid="management-center"');
    expect(source).toContain('className="grid w-full shrink-0 grid-cols-4');
    expect(source).toContain("总览");
    expect(source).toContain("笔记");
    expect(source).toContain("知识库");
    expect(source).toContain("AI");
    expect(source).not.toContain('{ id: "workspace"');
    expect(source).not.toContain('{ id: "security"');
    expect(source).not.toContain('{ id: "about"');
    expect(source).toContain("系统边界");
    expect(source).toContain("关于 Iris");
    expect(source).toContain("Iris");
    expect(source).toContain("版本 1.1.0");
    expect(source).toContain("GNU Affero General Public License v3.0");
    expect(source).not.toContain("开发者水印");
  });
});
