import { describe, expect, it } from "vitest";

import {
  patchSpansPreferSidebar,
  shouldUpgradeToSidebarDiff,
} from "@/lib/assistant-patch";

describe("shouldUpgradeToSidebarDiff", () => {
  it("keeps short rewrites inline-friendly", () => {
    expect(
      shouldUpgradeToSidebarDiff({
        originalLength: 80,
        replacementLength: 120,
        lineDelta: 2,
      }),
    ).toBe(false);
  });

  it("upgrades long replacements to sidebar diff", () => {
    expect(
      shouldUpgradeToSidebarDiff({
        originalLength: 120,
        replacementLength: 600,
        lineDelta: 3,
      }),
    ).toBe(true);
  });

  it("upgrades multi-line structural edits", () => {
    expect(
      shouldUpgradeToSidebarDiff({
        originalLength: 200,
        replacementLength: 350,
        lineDelta: 14,
      }),
    ).toBe(true);
  });
});

describe("patchSpansPreferSidebar", () => {
  it("detects when any patch should render in the assistant sidebar", () => {
    expect(
      patchSpansPreferSidebar([
        {
          original_text: "短句",
          replacement_text: "另一句",
        },
        {
          original_text: "a".repeat(40),
          replacement_text: "b".repeat(520),
        },
      ]),
    ).toBe(true);
  });
});
