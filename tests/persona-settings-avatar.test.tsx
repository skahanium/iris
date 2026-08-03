import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { PersonaSettingsBody } from "@/components/settings/PersonaSettingsPanel";
import { DEFAULT_PROMPT_PROFILE } from "@/lib/prompt-profile";

const mocks = vi.hoisted(() => ({
  saveProfile: vi.fn().mockResolvedValue(undefined),
  promptProfileGet: vi.fn(),
  promptProfilePresets: vi.fn(),
}));

vi.mock("@/hooks/usePromptProfile", () => ({
  usePromptProfile: () => ({ saveProfile: mocks.saveProfile }),
}));

vi.mock("@/lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/ipc")>()),
  promptProfileGet: mocks.promptProfileGet,
  promptProfilePresets: mocks.promptProfilePresets,
}));

describe("PersonaSettingsBody geometric avatars", () => {
  beforeEach(() => {
    mocks.saveProfile.mockClear();
    mocks.promptProfileGet.mockReset();
    mocks.promptProfileGet.mockResolvedValue(DEFAULT_PROMPT_PROFILE);
    mocks.promptProfilePresets.mockReset();
    mocks.promptProfilePresets.mockResolvedValue([]);
  });

  it("offers only eight geometric marks and saves the selected avatar id", async () => {
    render(<PersonaSettingsBody open />);

    await waitFor(() =>
      expect(mocks.promptProfileGet).toHaveBeenCalledTimes(1),
    );
    expect(screen.queryByLabelText("头像（emoji，可选）")).toBeNull();
    expect(screen.getAllByRole("button", { name: /^头像 / })).toHaveLength(8);

    const lens = screen.getByRole("button", { name: "头像 透镜" });
    fireEvent.click(lens);
    expect(lens.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: "保存角色倾向" }));
    await waitFor(() =>
      expect(mocks.saveProfile).toHaveBeenCalledWith(
        expect.objectContaining({ avatar_id: "lens" }),
      ),
    );
  });
});
