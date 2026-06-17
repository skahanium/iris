import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

describe("AiMessageBubble mention metadata", () => {
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

  it("renders user document mentions as a light metadata row outside the message body", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "user",
          content: "根据问题线索情况，请给出核查思路",
          mentions: [
            {
              raw: "@[问题线索工作思路（WY）.md]",
              value: "问题线索工作思路（WY）.md",
              kind: "file",
              label: "问题线索工作思路（WY）.md",
            },
            {
              raw: "@[线索附件/]",
              value: "线索附件/",
              kind: "folder",
              label: "线索附件",
            },
            {
              raw: "@[核查依据/初稿.md]",
              value: "核查依据/初稿.md",
              kind: "file",
              label: "核查依据/初稿.md",
            },
          ],
        }),
      );
    });

    expect(host.textContent).toContain("引用：");
    expect(host.textContent).toContain("问题线索工作思路（WY）.md");
    expect(host.textContent).toContain("线索附件");
    expect(host.textContent).toContain("+1");
    expect(host.textContent).toContain("根据问题线索情况，请给出核查思路");
    expect(host.textContent).not.toContain("@[");

    const metadata = host.querySelector("[data-ai-message-mentions]");
    expect(metadata).not.toBeNull();
    expect(metadata?.getAttribute("title")).toContain("核查依据/初稿.md");
  });
});
