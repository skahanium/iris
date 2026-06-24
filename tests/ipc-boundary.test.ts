import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = process.cwd();

function read(path: string): string {
  return readFileSync(join(root, path), "utf8");
}

function sourceFiles(dir: string): string[] {
  return readdirSync(join(root, dir), { withFileTypes: true }).flatMap(
    (entry) => {
      const path = `${dir}/${entry.name}`;
      if (entry.isDirectory()) return sourceFiles(path);
      return /\.(ts|tsx)$/.test(entry.name) ? [path] : [];
    },
  );
}

describe("IPC boundary", () => {
  it("keeps direct Tauri invoke calls inside src/lib/ipc.ts", () => {
    const directInvokeFiles = sourceFiles("src").filter((path) =>
      /\binvoke\s*\(/.test(read(path)),
    );

    expect(directInvokeFiles).toEqual(["src/lib/ipc.ts"]);
  });

  it("exposes registered maintenance commands through typed wrappers", () => {
    const ipc = read("src/lib/ipc.ts");
    const llmCommands = read("src-tauri/src/commands/llm.rs");

    expect(ipc).toContain("export async function settingsReset");
    expect(ipc).toContain('invoke("settings_reset"');
    expect(ipc).toContain("export async function versionCleanup");
    expect(ipc).toContain('invoke<number>("version_cleanup_cmd"');
    expect(llmCommands).toContain("Deprecated compatibility alias");
  });
});
