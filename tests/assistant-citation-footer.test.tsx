import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { AssistantCitationFooter } from "@/components/ai/AssistantCitationFooter";

afterEach(() => {
  cleanup();
});

describe("AssistantCitationFooter", () => {
  it("lists only citations referenced in the answer body", () => {
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

    expect(screen.getByText("来源")).toBeInTheDocument();
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

    expect(screen.getByText("来源")).toBeInTheDocument();
    expect(screen.getByText("Match report")).toBeInTheDocument();
    expect(screen.getByText("Coach news")).toBeInTheDocument();
    expect(screen.queryByText("Unused")).not.toBeInTheDocument();
  });
});
