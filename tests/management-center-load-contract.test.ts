import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

describe("management center load contract", () => {
  it("reuses app-level connectivity and does not refresh web providers on open", () => {
    const source = readFileSync(
      path.join(
        process.cwd(),
        "src/components/settings/ManagementCenterPanel.tsx",
      ),
      "utf8",
    );

    expect(source).not.toContain("useConnectivityStatus()");
    expect(source).not.toContain(
      "if (open) void onRefreshWebSearchProviders()",
    );
    expect(source).toContain("connectivityStatus");
  });

  it("preloads the management center bundle after app startup", () => {
    const appSource = readFileSync(
      path.join(process.cwd(), "src/App.impl.tsx"),
      "utf8",
    );

    expect(appSource).toContain("preloadManagementCenter");
  });
});
