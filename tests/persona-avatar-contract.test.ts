import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("persona avatar visual contract", () => {
  it("documents Iris geometric marks and the absence of a sidecar idle badge", () => {
    const design = read("docs/design-system.md");
    const roadmap = read("ROADMAP.md");
    const checklist = read(
      "docs/testing/iris-rail-refresh-manual-checklist.md",
    );

    expect(design).toContain("8 个内置灰阶几何印记");
    expect(design).toContain("不使用 emoji、插画、上传头像");
    expect(design).toContain("不显示空闲状态徽章");
    expect(roadmap).toContain("灰阶几何印记");
    expect(checklist).toContain("人格头像");
    expect(checklist).toContain("准备就绪");
  });
});
