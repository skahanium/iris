import { describe, expect, it } from "vitest";

import { skillConfirmSuccessNotice } from "@/lib/skill-install-notice";

describe("skillConfirmSuccessNotice", () => {
  it("prefers confirmed skill from backend response", () => {
    expect(
      skillConfirmSuccessNotice({
        confirmedSkill: "scrapling",
        preview: { display_name: "Scrapling" },
      }),
    ).toBe("已确认 Skill「scrapling」，可在设置 → Skills 查看。");
  });

  it("falls back to preview display_name", () => {
    expect(
      skillConfirmSuccessNotice({
        preview: { display_name: "Scrapling" },
        arguments: { name: "scrapling" },
      }),
    ).toBe("已确认 Skill「Scrapling」，可在设置 → Skills 查看。");
  });

  it("returns null when no skill name is available", () => {
    expect(skillConfirmSuccessNotice({})).toBeNull();
  });
});
