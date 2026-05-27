import { describe, expect, it } from "vitest";

import {
  IRIS_OVERLAY_SIZE_CLASS,
  irisOverlayPanelClass,
} from "@/lib/overlay-sizes";

describe("Iris overlay sizes", () => {
  it("maps command overlay sizes to centered viewport dimensions", () => {
    expect(IRIS_OVERLAY_SIZE_CLASS.compact).toContain("max-w-xl");
    expect(IRIS_OVERLAY_SIZE_CLASS.palette).toContain("max-w-2xl");
    expect(IRIS_OVERLAY_SIZE_CLASS.command).toContain("w-[80vw]");
    expect(IRIS_OVERLAY_SIZE_CLASS.command).toContain("h-[78vh]");
    expect(IRIS_OVERLAY_SIZE_CLASS.wide).toContain("w-[92vw]");
    expect(IRIS_OVERLAY_SIZE_CLASS.wide).toContain("h-[88vh]");
    expect(IRIS_OVERLAY_SIZE_CLASS["near-full"]).toContain("w-[92vw]");
    expect(IRIS_OVERLAY_SIZE_CLASS.graph).toContain("w-[96vw]");
    expect(IRIS_OVERLAY_SIZE_CLASS.graph).toContain("h-[92vh]");
  });

  it("combines the shared panel shell with the selected size", () => {
    const className = irisOverlayPanelClass("command", "custom-class");

    expect(className).toContain("fixed left-1/2 top-1/2");
    expect(className).toContain("z-overlay");
    expect(className).toContain("rounded-xl");
    expect(className).toContain("shadow-overlay");
    expect(className).toContain("w-[80vw]");
    expect(className).toContain("custom-class");
  });
});
