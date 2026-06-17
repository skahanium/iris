import { describe, expect, it } from "vitest";

import {
  classifiedBreadcrumbs,
  classifiedDisplayName,
} from "@/lib/classified-path";

describe("classified display helpers", () => {
  it("hides the internal classified root from display names", () => {
    expect(classifiedDisplayName(".classified")).toBe("保险库");
    expect(classifiedDisplayName(".classified/secret.md")).toBe("secret.md");
    expect(classifiedDisplayName(".classified/inbox/note.md")).toBe("note.md");
  });

  it("uses user-facing breadcrumbs without exposing internal paths", () => {
    const root = classifiedBreadcrumbs(".classified");
    const nested = classifiedBreadcrumbs(".classified/inbox/archive");

    expect(root).toEqual([{ label: "保险库", path: ".classified" }]);
    expect(nested.map((crumb) => crumb.label)).toEqual([
      "保险库",
      "inbox",
      "archive",
    ]);
    expect(nested.map((crumb) => crumb.label).join("/")).not.toContain(
      ".classified",
    );
  });
});
