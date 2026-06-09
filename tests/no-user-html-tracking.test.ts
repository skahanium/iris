import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("user generated HTML artifacts", () => {
  it("ignores exported note HTML under src-tauri", () => {
    const gitignore = readFileSync(".gitignore", "utf8");

    expect(gitignore).toContain("src-tauri/*.html");
  });

  it("does not track exported note HTML under src-tauri", () => {
    const trackedHtml = execFileSync("git", ["ls-files", "src-tauri/*.html"], {
      encoding: "utf8",
    })
      .split(/\r?\n/)
      .filter(Boolean);

    expect(trackedHtml).toEqual([]);
  });
});
