import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function packageVersion(): string {
  const pkg = JSON.parse(read("package.json")) as { version: string };
  return pkg.version;
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
    const aboutLine =
      source
        .split("\n")
        .find((line) =>
          line.includes("GNU Affero General Public License v3.0"),
        ) ?? "";
    expect(aboutLine).toContain(packageVersion());
    expect(source).toContain("GNU Affero General Public License v3.0");
    expect(source).not.toContain("开发者水印");
  });

  it("keeps the authorized-material disclosure beside the Web switch", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(source).toContain(
      "联网时，显式附带材料中的检索主题可能发送给所选搜索服务",
    );
  });
});
