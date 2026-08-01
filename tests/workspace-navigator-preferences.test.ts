import { afterEach, describe, expect, it } from "vitest";

import {
  DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES,
  loadWorkspaceNavigatorPreferences,
  saveWorkspaceNavigatorPreferences,
} from "@/lib/workspace-navigator-preferences";

describe("workspace navigator preferences", () => {
  afterEach(() => {
    localStorage.clear();
  });

  it("持久化不包含路径的布局和显示偏好", () => {
    saveWorkspaceNavigatorPreferences({
      dividerPercent: 65,
      fileSort: { direction: "desc", key: "updatedAt" },
      folderSort: { direction: "desc", key: "count" },
      showMedia: true,
    });

    expect(loadWorkspaceNavigatorPreferences()).toEqual({
      dividerPercent: 65,
      fileSort: { direction: "desc", key: "updatedAt" },
      folderSort: { direction: "desc", key: "count" },
      showMedia: true,
    });
    expect(
      localStorage.getItem("iris.workspaceNavigator.preferences"),
    ).not.toContain("notes/");
  });

  it("非法或越界缓存安全回退到默认值", () => {
    localStorage.setItem(
      "iris.workspaceNavigator.preferences",
      JSON.stringify({ dividerPercent: 99, fileSort: { key: "bad" } }),
    );

    expect(loadWorkspaceNavigatorPreferences()).toEqual(
      DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES,
    );
  });
});
