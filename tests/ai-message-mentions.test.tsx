import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

describe("AiMessageBubble inline display mentions", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("restores validated mentions inline without @, brackets, chips or a reference row", async () => {
    const content = "请根据 Guide 与 Notes 给出思路";
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "user",
          content,
          displayMentions: [
            {
              kind: "file",
              value: "Policies/Guide.md",
              label: "Guide",
              range: { from: 4, to: 9 },
            },
            {
              kind: "folder",
              value: "Research/Notes/",
              label: "Notes",
              range: { from: 12, to: 17 },
            },
          ],
        }),
      );
    });

    const mentions = host.querySelectorAll(".ai-display-mention");
    expect(mentions).toHaveLength(2);
    expect(mentions[0]?.textContent).toBe("Guide");
    expect(mentions[0]?.getAttribute("title")).toBe("文档：Policies/Guide.md");
    expect(mentions[1]?.getAttribute("title")).toBe("文件夹：Research/Notes/");
    expect(host.textContent?.trim()).toBe(content);
    expect(host.textContent).not.toContain("引用：");
    expect(host.textContent).not.toMatch(/@|\[|\]/);
    expect(host.querySelector("[data-ai-message-mentions]")).toBeNull();
  });

  it("renders stale or mismatched metadata as ordinary text", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "user",
          content: "请根据 Guide",
          displayMentions: [
            {
              kind: "file",
              value: '<img src=x onerror="alert(1)">',
              label: "Other",
              range: { from: 4, to: 9 },
            },
          ],
        }),
      );
    });

    expect(host.textContent?.trim()).toBe("请根据 Guide");
    expect(host.querySelector(".ai-display-mention")).toBeNull();
    expect(host.querySelector("img")).toBeNull();
  });

  it("does not expose an unsafe absolute path through the tooltip", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "user",
          content: "查 Guide",
          displayMentions: [
            {
              kind: "file",
              value: "/Users/example/private/Guide.md",
              label: "Guide",
              range: { from: 2, to: 7 },
            },
          ],
        }),
      );
    });

    const mention = host.querySelector(".ai-display-mention");
    expect(mention?.getAttribute("title")).toBe("文档");
    expect(host.innerHTML).not.toContain("/Users/example/private");
  });

  it("only upgrades placeholders in rendered Markdown text nodes", async () => {
    const content = [
      "普通 Guide",
      "[链接](Guide)",
      "`Guide`",
      "<mark>Guide</mark>",
    ].join("\n\n");
    const guideStarts = Array.from(
      content.matchAll(/Guide/g),
      (match) => match.index ?? -1,
    );

    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "user",
          content,
          displayMentions: guideStarts.map((from) => ({
            kind: "file" as const,
            value: "Policies/Guide.md",
            label: "Guide",
            range: { from, to: from + "Guide".length },
          })),
        }),
      );
    });

    const mentions = host.querySelectorAll(".ai-display-mention");
    expect(mentions).toHaveLength(1);
    expect(mentions[0]?.parentElement?.tagName).toBe("P");

    const link = host.querySelector<HTMLAnchorElement>("a");
    expect(link?.textContent).toBe("链接");
    expect(link?.getAttribute("href")).toBe("Guide");
    expect(link?.querySelector(".ai-display-mention")).toBeNull();

    const code = host.querySelector("code");
    expect(code?.textContent).toBe("Guide");
    expect(code?.querySelector(".ai-display-mention")).toBeNull();

    const rawHtml = host.querySelector("mark");
    expect(rawHtml?.textContent).toBe("Guide");
    expect(rawHtml?.querySelector(".ai-display-mention")).toBeNull();
    expect(host.innerHTML).not.toContain("IRISDISPLAYMENTIONPLACEHOLDER");
  });
});
