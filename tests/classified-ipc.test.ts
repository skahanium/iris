import { readFileSync } from "node:fs";

import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

import {
  classifiedDelete,
  classifiedExport,
  classifiedFiles,
  classifiedImport,
  classifiedLock,
  classifiedMkdir,
  classifiedRename,
  classifiedSetup,
  classifiedStatus,
  classifiedUnlock,
  fileRead,
  fileSetLock,
} from "@/lib/ipc";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const CLASSIFIED_COMMANDS = [
  "classified_setup",
  "classified_unlock",
  "classified_lock",
  "classified_status",
  "classified_files",
  "classified_import",
  "classified_export",
  "classified_delete",
  "classified_mkdir",
  "classified_rename",
] as const;

describe("classified vault IPC contract", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("registers all classified commands in Tauri lib.rs", () => {
    const lib = read("src-tauri/src/lib.rs");
    for (const cmd of CLASSIFIED_COMMANDS) {
      expect(lib).toContain(`commands::classified::${cmd}`);
    }
  });

  it("defines ClassifiedStatus and ClassifiedFileEntry in types/ipc.ts", () => {
    const types = read("src/types/ipc.ts");
    expect(types).toContain("export interface FileReadResult");
    expect(types).toContain("isLocked: boolean");
    expect(types).toContain("export interface ClassifiedFileEntry");
    expect(types).toContain("isDir: boolean");
    expect(types).toContain('export type ClassifiedStatus =');
    expect(types).toContain('"needs_setup"');
    expect(types).toContain('"waiting"');
  });

  it("classifiedSetup invokes backend with password", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedSetup("secret");
    expect(invoke).toHaveBeenCalledWith("classified_setup", {
      password: "secret",
    });
  });

  it("classifiedUnlock invokes backend with password", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedUnlock("secret");
    expect(invoke).toHaveBeenCalledWith("classified_unlock", {
      password: "secret",
    });
  });

  it("classifiedLock invokes backend without extra args", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedLock();
    expect(invoke).toHaveBeenCalledWith("classified_lock");
  });

  it("classifiedStatus returns typed status string", async () => {
    invoke.mockResolvedValue("unlocked");
    await expect(classifiedStatus()).resolves.toBe("unlocked");
    expect(invoke).toHaveBeenCalledWith("classified_status");
  });

  it("classifiedFiles passes null folder when omitted", async () => {
    invoke.mockResolvedValue([]);
    await classifiedFiles();
    expect(invoke).toHaveBeenCalledWith("classified_files", { folder: null });
  });

  it("classifiedFiles passes folder when provided", async () => {
    invoke.mockResolvedValue([]);
    await classifiedFiles("inbox");
    expect(invoke).toHaveBeenCalledWith("classified_files", {
      folder: "inbox",
    });
  });

  it("classifiedImport maps camelCase targetFolder to IPC", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedImport("notes/a.md", ".classified/inbox");
    expect(invoke).toHaveBeenCalledWith("classified_import", {
      path: "notes/a.md",
      targetFolder: ".classified/inbox",
    });
  });

  it("classifiedExport maps camelCase targetFolder to IPC", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedExport(".classified/secret.md", "notes");
    expect(invoke).toHaveBeenCalledWith("classified_export", {
      path: ".classified/secret.md",
      targetFolder: "notes",
    });
  });

  it("classifiedDelete invokes backend with path", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedDelete(".classified/secret.md");
    expect(invoke).toHaveBeenCalledWith("classified_delete", {
      path: ".classified/secret.md",
    });
  });

  it("classifiedMkdir invokes backend with folder", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedMkdir(".classified/inbox");
    expect(invoke).toHaveBeenCalledWith("classified_mkdir", {
      folder: ".classified/inbox",
    });
  });

  it("classifiedRename invokes backend with path and newPath", async () => {
    invoke.mockResolvedValue(undefined);
    await classifiedRename(".classified/a.md", ".classified/b.md");
    expect(invoke).toHaveBeenCalledWith("classified_rename", {
      path: ".classified/a.md",
      newPath: ".classified/b.md",
    });
  });

  it("fileSetLock invokes backend with path and locked flag", async () => {
    invoke.mockResolvedValue(undefined);
    await fileSetLock("notes/a.md", true);
    expect(invoke).toHaveBeenCalledWith("file_set_lock", {
      path: "notes/a.md",
      locked: true,
    });
  });

  it("fileRead returns FileReadResult shape from invoke", async () => {
    invoke.mockResolvedValue({ content: "# Hi", isLocked: false });
    await expect(fileRead("notes/a.md")).resolves.toEqual({
      content: "# Hi",
      isLocked: false,
    });
    expect(invoke).toHaveBeenCalledWith("file_read", { path: "notes/a.md" });
  });
});

describe("fileRead call-site compatibility (Task 15)", () => {
  it("useTabManager destructures content and isLocked from fileRead", () => {
    const source = read("src/hooks/useTabManager.ts");
    expect(source).toMatch(
      /\{\s*content,\s*isLocked\s*\}\s*=\s*await fileRead\(/,
    );
  });

  it("note-tab-lifecycle destructures content from fileRead", () => {
    const source = read("src/lib/note-tab-lifecycle.ts");
    expect(source).toMatch(/\{\s*content\s*\}\s*=\s*await fileRead\(/);
  });

  it("VaultNavigator destructures content from fileRead", () => {
    const source = read("src/components/file/VaultNavigator.tsx");
    expect(source).toMatch(/\{\s*content:\s*md\s*\}\s*=\s*await fileRead\(/);
  });

  it("App.tsx destructures externalContent from fileRead", () => {
    const source = read("src/App.tsx");
    expect(source).toMatch(
      /\.then\(\(\{\s*content:\s*externalContent\s*\}\)/,
    );
  });
});
