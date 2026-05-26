import { describe, expect, it } from "vitest";

import {
  closeOverlayState,
  openOverlayState,
  toggleOverlayState,
  type OverlayState,
} from "@/hooks/useOverlayManager";

describe("useOverlayManager state transitions", () => {
  it("opens a single active overlay and closes the previous one", () => {
    let state: OverlayState = { activeOverlay: null };

    state = openOverlayState(state, "quickOpen");
    expect(state.activeOverlay).toBe("quickOpen");

    state = openOverlayState(state, "search");
    expect(state.activeOverlay).toBe("search");
  });

  it("toggles an open overlay closed and a closed overlay open", () => {
    let state: OverlayState = { activeOverlay: "graph" };

    state = toggleOverlayState(state, "graph");
    expect(state.activeOverlay).toBeNull();

    state = toggleOverlayState(state, "version");
    expect(state.activeOverlay).toBe("version");
  });

  it("closes only the matching overlay when an id is supplied", () => {
    const openState: OverlayState = { activeOverlay: "settings" };

    expect(closeOverlayState(openState, "search").activeOverlay).toBe(
      "settings",
    );
    expect(closeOverlayState(openState, "settings").activeOverlay).toBeNull();
    expect(closeOverlayState(openState).activeOverlay).toBeNull();
  });
});
