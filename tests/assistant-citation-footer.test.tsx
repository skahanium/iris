import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { AssistantCitationFooter } from "@/components/ai/AssistantCitationFooter";

afterEach(() => {
  cleanup();
});

describe("AssistantCitationFooter", () => {
  it("keeps referenced citations collapsed until the source summary is expanded", () => {
    render(
      <AssistantCitationFooter
        content="葡萄牙晋级 [1]，教练辞职 [3]。"
        entries={[
          { index: 1, title: "Match report", url: "https://example.com/one" },
          { index: 2, title: "Unused", url: "https://example.com/two" },
          { index: 3, title: "Coach news", url: "https://example.com/three" },
        ]}
      />,
    );

    const toggle = screen.getByRole("button", { name: "展开来源" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("2 个来源")).toBeInTheDocument();
    expect(screen.queryByText("Match report")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Match report")).toBeInTheDocument();
    expect(screen.getByText("Coach news")).toBeInTheDocument();
    expect(screen.queryByText("Unused")).not.toBeInTheDocument();
  });

  it("lists citations when body only has linkified markdown footnotes", () => {
    render(
      <AssistantCitationFooter
        content="葡萄牙晋级 [1](https://example.com/one)，教练辞职 [3](https://example.com/three)。"
        entries={[
          { index: 1, title: "Match report", url: "https://example.com/one" },
          { index: 2, title: "Unused", url: "https://example.com/two" },
          { index: 3, title: "Coach news", url: "https://example.com/three" },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "展开来源" }));
    expect(screen.getByText("Match report")).toBeInTheDocument();
    expect(screen.getByText("Coach news")).toBeInTheDocument();
    expect(screen.queryByText("Unused")).not.toBeInTheDocument();
  });

  it("uses a neutral verified-source group when a provider omitted markers", () => {
    render(
      <AssistantCitationFooter
        content="模型回答没有行内引用格式。"
        binding={{
          mode: "source_group_fallback",
          referencedIndices: [],
          fallbackReason: "missing_marker",
        }}
        entries={[
          { index: 1, title: "Verified one", url: "https://example.com/one" },
          { index: 2, title: "Verified two", url: "https://example.com/two" },
        ]}
      />,
    );

    const toggle = screen.getByRole("button", {
      name: "展开本轮已核验证据",
    });
    expect(screen.getByText("2 项证据")).toBeInTheDocument();
    expect(screen.queryByText("Verified one")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    expect(screen.getByText("本轮已核验证据")).toBeInTheDocument();
    expect(screen.getByText("Verified one")).toBeInTheDocument();
    expect(screen.getByText("Verified two")).toBeInTheDocument();
    expect(screen.getByText(/未提供可精确绑定/)).toBeInTheDocument();
  });
});
