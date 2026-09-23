import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AssistantConfirmationDiff } from "@/components/ai/AssistantConfirmationDiff";
import { assistantRunConfirmationDiff } from "@/lib/ipc";
import type {
  AssistantRunConfirmationDiffRequest,
  ConfirmationDiffPreview,
} from "@/types/ai";

vi.mock("@/lib/ipc", () => ({
  assistantRunConfirmationDiff: vi.fn(),
}));

const request: AssistantRunConfirmationDiffRequest = {
  session: { domain: "normal", sessionKey: "session-1" },
  runId: "run-1",
  confirmationId: "conf-1",
  planHash: "sha256:plan",
};

function previewFixture(
  overrides: Partial<ConfirmationDiffPreview> = {},
): ConfirmationDiffPreview {
  return {
    files: [
      {
        path: "notes/a.md",
        previewable: true,
        hunks: [
          {
            oldStart: 1,
            newStart: 1,
            lines: [
              { kind: "context", text: "a" },
              { kind: "del", text: "b" },
              { kind: "add", text: "B" },
              { kind: "context", text: "c" },
            ],
          },
        ],
      },
    ],
    truncated: false,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  vi.mocked(assistantRunConfirmationDiff).mockReset();
});

describe("AssistantConfirmationDiff", () => {
  it("stays collapsed and fetches nothing until expanded", () => {
    render(<AssistantConfirmationDiff request={request} />);

    const toggle = screen.getByRole("button", { name: "展开更改差异" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(assistantRunConfirmationDiff).not.toHaveBeenCalled();
  });

  it("fetches once and renders unified diff lines on expand", async () => {
    vi.mocked(assistantRunConfirmationDiff).mockResolvedValue(previewFixture());
    render(<AssistantConfirmationDiff request={request} />);

    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));

    expect(
      screen.getByRole("button", { name: "折叠更改差异" }),
    ).toHaveAttribute("aria-expanded", "true");
    await waitFor(() => {
      expect(screen.getByText("notes/a.md")).toBeInTheDocument();
    });
    expect(assistantRunConfirmationDiff).toHaveBeenCalledTimes(1);
    expect(assistantRunConfirmationDiff).toHaveBeenCalledWith(request);
    expect(screen.getByText("−b")).toBeInTheDocument();
    expect(screen.getByText("+B")).toBeInTheDocument();
    expect(
      screen.getByText((_content, element) => element?.textContent === " a"),
    ).toBeInTheDocument();
  });

  it("does not refetch after collapse and re-expand", async () => {
    vi.mocked(assistantRunConfirmationDiff).mockResolvedValue(previewFixture());
    render(<AssistantConfirmationDiff request={request} />);

    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));
    await waitFor(() => {
      expect(screen.getByText("notes/a.md")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "折叠更改差异" }));
    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));

    expect(assistantRunConfirmationDiff).toHaveBeenCalledTimes(1);
  });

  it("marks a truncated preview", async () => {
    vi.mocked(assistantRunConfirmationDiff).mockResolvedValue(
      previewFixture({ truncated: true }),
    );
    render(<AssistantConfirmationDiff request={request} />);

    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));

    await waitFor(() => {
      expect(
        screen.getByText("差异已截断，仅显示部分内容"),
      ).toBeInTheDocument();
    });
  });

  it("falls back quietly when the preview fails", async () => {
    vi.mocked(assistantRunConfirmationDiff).mockRejectedValue(
      new Error("backend unavailable"),
    );
    render(<AssistantConfirmationDiff request={request} />);

    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));

    await waitFor(() => {
      expect(screen.getByText("差异暂不可用")).toBeInTheDocument();
    });
    expect(screen.queryByText("notes/a.md")).not.toBeInTheDocument();
  });

  it("reports unpreviewable targets without hunks", async () => {
    vi.mocked(assistantRunConfirmationDiff).mockResolvedValue({
      files: [{ path: "notes/new.md", previewable: false, hunks: [] }],
      truncated: false,
    });
    render(<AssistantConfirmationDiff request={request} />);

    fireEvent.click(screen.getByRole("button", { name: "展开更改差异" }));

    await waitFor(() => {
      expect(screen.getByText("notes/new.md")).toBeInTheDocument();
    });
    expect(screen.getByText("此目标无法预览差异")).toBeInTheDocument();
    expect(screen.queryByText("−b")).not.toBeInTheDocument();
  });
});
