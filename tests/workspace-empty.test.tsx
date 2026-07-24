import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceEmpty } from "@/components/layout/WorkspaceEmpty";

describe("WorkspaceEmpty", () => {
  it("vault mode shows create-first brand CTA only", async () => {
    const onNew = vi.fn();
    render(<WorkspaceEmpty mode="vault" onNew={onNew} />);
    expect(screen.getByTestId("workspace-empty")).toHaveAttribute(
      "data-mode",
      "vault",
    );
    expect(screen.queryByTestId("workspace-empty-open-recent")).toBeNull();
    await userEvent.click(screen.getByTestId("workspace-empty-new"));
    expect(onNew).toHaveBeenCalled();
  });

  it("workspace mode can open recent via weak link", async () => {
    const onOpenRecent = vi.fn();
    render(
      <WorkspaceEmpty
        mode="workspace"
        onNew={vi.fn()}
        onOpenRecent={onOpenRecent}
      />,
    );
    await userEvent.click(screen.getByTestId("workspace-empty-open-recent"));
    expect(onOpenRecent).toHaveBeenCalled();
  });
});
