import { describe, expect, it } from "vitest";

import { skillInstallSuccessNotice } from "@/lib/skill-install-notice";

describe("skillInstallSuccessNotice", () => {
  it("prefers installed_skill from backend response", () => {
    expect(
      skillInstallSuccessNotice({
        installedSkill: "scrapling",
        preview: { display_name: "Scrapling" },
      }),
    ).toBe("已安装 Skill「scrapling」，可在设置 → Skills 查看。");
  });

  it("falls back to preview display_name", () => {
    expect(
      skillInstallSuccessNotice({
        preview: { display_name: "Scrapling" },
        arguments: { path_or_url: "scrapling" },
      }),
    ).toBe("已安装 Skill「Scrapling」，可在设置 → Skills 查看。");
  });

  it("returns null when no skill name is available", () => {
    expect(skillInstallSuccessNotice({})).toBeNull();
  });
});
