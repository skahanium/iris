import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function testSourceFiles(root: string): string[] {
  const entries = readdirSync(root, { withFileTypes: true });
  const files: string[] = [];

  for (const entry of entries) {
    const path = `${root}/${entry.name}`;
    if (entry.isDirectory()) {
      files.push(...testSourceFiles(path));
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry.name)) files.push(path);
  }

  return files;
}

const checkedUserFacingFiles = [
  "src/components/ai/AgentStatusBadge.tsx",
  "src/components/ai/SkillsPanel.tsx",
  "src/components/ai/UnifiedAssistantPanel.impl.tsx",
  "src/components/ai/hooks/useAssistantConversation.ts",
  "src/hooks/useAssistantRun.ts",
  "src/hooks/useInlineAi.ts",
  "src/lib/ipc.ts",
  "src/types/ipc.ts",
];

describe("repository text hygiene", () => {
  it("pins repository text files to LF line endings", () => {
    const attrs = read(".gitattributes");

    expect(attrs).toContain("* text=auto eol=lf");
    expect(attrs).toContain("*.bat text eol=crlf");
    expect(attrs).toContain("*.cmd text eol=crlf");
    expect(attrs).toContain("*.ps1 text eol=crlf");
  });

  it("pins Prettier output to LF to avoid Windows autocrlf churn", () => {
    const prettierConfig = JSON.parse(read(".prettierrc")) as {
      endOfLine?: string;
    };

    expect(prettierConfig.endOfLine).toBe("lf");
  });

  it("does not keep stale TDD failure labels in green test names", () => {
    const staleLabel = "[TDD" + "-FAIL]";
    const offenders = testSourceFiles("tests").filter((path) =>
      read(path).includes(staleLabel),
    );

    expect(offenders).toEqual([]);
  });

  it("does not expose mojibake in AI-facing UI and IPC contract text", () => {
    const mojibakePattern =
      /[鈹€鍘熸枃鐮旂┒缁煎悎鍐欀綔寮曠敤鏍告煡杞婚噺瀵硅瘽鐘舵€褰撳墠瀹夎璺緞鏃ф牸闇€纭娑堟伅宸插鍒堕€変腑澶嶅埗澶辫触鎾ゅ洖鏈悓姝ュ埌浼氳瘽璁板綍]/;
    const offenders = checkedUserFacingFiles.filter((path) =>
      mojibakePattern.test(read(path)),
    );

    expect(offenders).toEqual([]);
  });
});
