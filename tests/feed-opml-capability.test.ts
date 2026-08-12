import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("OPML 文件访问 capability", () => {
  it("只开放对话框动态授权路径所需的文本读写命令", () => {
    const capability = JSON.parse(
      readFileSync("src-tauri/capabilities/default.json", "utf8"),
    ) as { permissions: unknown[] };

    expect(capability.permissions).toContain("fs:allow-read-text-file");
    expect(capability.permissions).toContain("fs:allow-write-text-file");
    expect(capability.permissions).not.toContain("fs:default");
  });
});
