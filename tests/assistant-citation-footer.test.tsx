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
        binding={{ mode: "exact", referencedIndices: [1, 3] }}
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
    expect(screen.queryByText("本次检索来源")).not.toBeInTheDocument();
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

  it("labels an uncalibrated source group as this-run retrieval sources", () => {
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
      name: "展开本次检索来源",
    });
    expect(screen.getByText("2 个来源")).toBeInTheDocument();
    expect(screen.queryByText("Verified one")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    expect(screen.getByText("本次检索来源")).toBeInTheDocument();
    expect(screen.getByText("Verified one")).toBeInTheDocument();
    expect(screen.getByText("Verified two")).toBeInTheDocument();
    expect(
      screen.getByText(
        "本回答未提供可精确绑定的行内引用；以下仅为本次检索来源，不表示已逐段核验。",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("本轮已核验证据")).not.toBeInTheDocument();
  });

  it("shows every registered source-group entry instead of imposing a five-source display cap", () => {
    const entries = Array.from({ length: 12 }, (_, index) => ({
      index: index + 1,
      title: `Source ${index + 1}`,
      url: `https://example.com/${index + 1}`,
    }));
    render(
      <AssistantCitationFooter
        content="模型回答没有行内引用格式。"
        binding={{
          mode: "source_group_fallback",
          referencedIndices: [],
          fallbackReason: "missing_marker",
        }}
        entries={entries}
      />,
    );

    expect(screen.getByText("12 个来源")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "展开本次检索来源" }));
    expect(screen.getByText("Source 1")).toBeInTheDocument();
    expect(screen.getByText("Source 12")).toBeInTheDocument();
  });

  it("treats a missing binding as this-run retrieval sources instead of precise citations", () => {
    render(
      <AssistantCitationFooter
        content="模型回答没有行内引用格式。"
        entries={[
          { index: 1, title: "Verified one", url: "https://example.com/one" },
          { index: 2, title: "Verified two", url: "https://example.com/two" },
        ]}
      />,
    );

    const toggle = screen.getByRole("button", {
      name: "展开本次检索来源",
    });
    fireEvent.click(toggle);

    expect(screen.getByText("本次检索来源")).toBeInTheDocument();
    expect(
      screen.getByText(
        "本回答未提供可精确绑定的行内引用；以下仅为本次检索来源，不表示已逐段核验。",
      ),
    ).toBeInTheDocument();
  });

  it("fails safe for an unknown binding version", () => {
    const binding = {
      mode: "future_unknown_version",
      referencedIndices: [],
    } as unknown as import("@/types/ai").CitationBinding;

    render(
      <AssistantCitationFooter
        content="模型回答没有行内引用格式。"
        binding={binding}
        entries={[
          { index: 1, title: "Verified one", url: "https://example.com/one" },
          { index: 2, title: "Verified two", url: "https://example.com/two" },
        ]}
      />,
    );

    const toggle = screen.getByRole("button", {
      name: "展开本次检索来源",
    });
    fireEvent.click(toggle);

    expect(screen.getByText("本次检索来源")).toBeInTheDocument();
    expect(
      screen.getByText(
        "本回答未提供可精确绑定的行内引用；以下仅为本次检索来源，不表示已逐段核验。",
      ),
    ).toBeInTheDocument();
  });

  it("shows only category counts until the source disclosure is expanded", () => {
    render(
      <AssistantCitationFooter
        content="基于本轮材料的建议。"
        entries={[]}
        sourceSummary={[
          { category: "user_input", count: 1 },
          { category: "authorized_material", count: 2 },
          { category: "web", count: 2 },
          { category: "model_inference", count: 1 },
        ]}
      />,
    );

    const toggle = screen.getByRole("button", { name: "展开来源" });
    expect(toggle).toHaveTextContent(
      "用户输入 1 · 授权材料 2 · 网页 2 · 推断 1",
    );

    fireEvent.click(toggle);
    expect(
      screen.getAllByText("用户输入 1 · 授权材料 2 · 网页 2 · 推断 1"),
    ).toHaveLength(2);
  });
});
