import { describe, expect, it } from "vitest";

/** Side panels must be mutually exclusive (same rule as useOverlayManager). */
const SIDE_PANELS = [
  "fileSheet",
  "search",
  "settings",
  "backlinks",
  "tags",
  "version",
] as const;

function openPanel(
  current: Record<string, boolean>,
  id: (typeof SIDE_PANELS)[number],
): Record<string, boolean> {
  const next = Object.fromEntries(
    SIDE_PANELS.map((k) => [k, k === id]),
  ) as Record<string, boolean>;
  return { ...current, ...next, graph: false };
}

describe("overlay panel mutex", () => {
  it("opening one side panel closes others", () => {
    let state = openPanel({}, "fileSheet");
    expect(state.fileSheet).toBe(true);
    expect(state.search).toBe(false);

    state = openPanel(state, "search");
    expect(state.fileSheet).toBe(false);
    expect(state.search).toBe(true);
  });
});
