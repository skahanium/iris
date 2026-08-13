import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { Sheet, SheetContent } from "@/components/ui/sheet";

afterEach(cleanup);

describe("SheetContent", () => {
  it("keeps the default sheet flush with the viewport", () => {
    render(
      <Sheet open>
        <SheetContent data-testid="default-sheet">内容</SheetContent>
      </Sheet>,
    );

    const sheet = screen.getByTestId("default-sheet");
    expect(sheet.className).toContain("inset-y-0");
    expect(sheet.className).not.toContain("top-[var(--titlebar-height)]");
  });

  it("supports keeping a sheet below the desktop titlebar", () => {
    render(
      <Sheet open>
        <SheetContent topInset="titlebar" data-testid="titlebar-sheet">
          内容
        </SheetContent>
      </Sheet>,
    );

    expect(screen.getByTestId("titlebar-sheet").className).toContain(
      "top-[var(--titlebar-height)]",
    );
  });
});
