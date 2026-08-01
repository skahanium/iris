import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { AssistantAvatar } from "@/components/ai/AssistantAvatar";

describe("AssistantAvatar", () => {
  it("renders the selected Iris geometric mark instead of an emoji or display-name initial", () => {
    render(
      <AssistantAvatar identity={{ displayName: "小鸢", avatarId: "lens" }} />,
    );

    const avatar = screen.getByTestId("assistant-avatar");
    expect(avatar.getAttribute("data-avatar-id")).toBe("lens");
    expect(avatar.querySelector("svg")).toBeTruthy();
    expect(avatar.textContent).toBe("");
  });
});
