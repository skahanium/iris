import { describe, expect, it } from "vitest";

import {
  closeOverlayState,
  openManagementCenterState,
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
    const openState: OverlayState = {
      activeOverlay: "managementCenter",
      managementCenterSection: "overview",
    };

    expect(closeOverlayState(openState, "search").activeOverlay).toBe(
      "managementCenter",
    );
    expect(
      closeOverlayState(openState, "managementCenter").activeOverlay,
    ).toBeNull();
    expect(closeOverlayState(openState).activeOverlay).toBeNull();
  });

  it("opens the management center with a section and optional detail target", () => {
    let state: OverlayState = { activeOverlay: "search" };

    state = openManagementCenterState(state, "ai", "skills");

    expect(state).toEqual({
      activeOverlay: "managementCenter",
      managementCenterSection: "ai",
      managementCenterDetail: "skills",
    });

    state = openOverlayState(state, "quickOpen");

    expect(state).toEqual({
      activeOverlay: "quickOpen",
      managementCenterSection: "overview",
      managementCenterDetail: null,
    });
  });
});
