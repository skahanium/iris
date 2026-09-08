import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { StreamingMessageBody } from "@/components/ai/StreamingMessageBody";
import * as markdownContract from "@/lib/markdown-contract";
import { StreamingLineBudgetRefContext } from "@/lib/streaming-line-budget-ref";
import type { StreamingLineBudget } from "@/lib/streaming-line-fit";

describe("StreamingMessageBody", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
    vi.restoreAllMocks();
  });

  it.each([
    {
      name: "code rows",
      initial: "```txt\nshort",
      expanded: "```txt\n" + "line();\n".repeat(2000),
    },
    {
      name: "Markdown blocks",
      initial: "first\n\n",
      expanded: Array.from(
        { length: 100 },
        (_, i) => `paragraph ${i}\n\n`,
      ).join(""),
    },
  ])(
    "does not reset the shared viewport when enabling $name",
    ({ initial, expanded }) => {
      host = document.createElement("div");
      host.setAttribute("data-radix-scroll-area-viewport", "");
      document.body.append(host);
      root = createRoot(host);
      const viewport = host;
      const scrollTo = vi.fn(
        (options?: ScrollToOptions | number, y?: number) => {
          viewport.scrollTop =
            typeof options === "number" ? (y ?? 0) : (options?.top ?? 0);
        },
      );
      viewport.scrollTo = scrollTo;
      act(() => root?.render(<StreamingMessageBody content={initial} />));
      viewport.scrollTop = 1200;
      scrollTo.mockClear();
      act(() => root?.render(<StreamingMessageBody content={expanded} />));
      expect(viewport.scrollTop).toBe(1200);
      expect(scrollTo).not.toHaveBeenCalled();
      expect(host.querySelector("[data-source-row]")).not.toBeNull();
    },
  );

  it.each([
    {
      name: "code rows",
      content: "```txt\n" + "A".repeat(80_000),
      top: 12_000,
      selector: "[data-source-row]",
      expectedIndex: 19,
    },
    {
      name: "Markdown blocks",
      content: Array.from({ length: 100 }, (_, i) => `paragraph ${i}\n\n`).join(
        "",
      ),
      top: 1200,
      selector: "[data-markdown-block]",
      expectedIndex: 33,
    },
  ])(
    "initializes $name at the existing viewport offset without a scroll event",
    ({ content, top, selector, expectedIndex }) => {
      host = document.createElement("div");
      host.setAttribute("data-radix-scroll-area-viewport", "");
      document.body.append(host);
      const viewport = host;
      Object.defineProperties(viewport, {
        offsetHeight: { value: 650 },
        offsetWidth: { value: 700 },
      });
      viewport.scrollTop = top;
      const scrollTo = vi.fn();
      viewport.scrollTo = scrollTo;
      vi.spyOn(
        HTMLElement.prototype,
        "getBoundingClientRect",
      ).mockImplementation(function (this: HTMLElement) {
        return {
          top: this === viewport ? 0 : -viewport.scrollTop,
          left: 0,
          bottom: 0,
          right: 700,
          width: 700,
          height: 0,
          x: 0,
          y: 0,
          toJSON: () => ({}),
        };
      });
      root = createRoot(host);
      act(() =>
        root?.render(
          <StreamingMessageBody content={content} streaming={false} />,
        ),
      );
      const indices = [...host.querySelectorAll(selector)].map((element) =>
        Number(element.closest("[data-index]")?.getAttribute("data-index")),
      );
      expect(indices).toContain(expectedIndex);
      expect(Math.min(...indices)).toBeGreaterThan(0);
      expect(viewport.scrollTop).toBe(top);
      expect(scrollTo).not.toHaveBeenCalled();
    },
  );

  it("appends newly stable blocks without replacing earlier rendered blocks", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n第二段。\n\n仍在输出"}
          contentIdentity="run-1"
        />,
      );
    });
    const first = host?.querySelector("p");
    expect(first?.textContent).toBe("第一段。");

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n第二段。\n\n第三段。\n\n仍在输出"}
          contentIdentity="run-1"
        />,
      );
    });

    const paragraphs = host?.querySelectorAll("p");
    expect(paragraphs).toHaveLength(4);
    expect(paragraphs?.[0]).toBe(first);
    expect(paragraphs?.[1]?.textContent).toBe("第二段。");
    expect(paragraphs?.[2]?.textContent).toBe("第三段。");
    expect(host?.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "仍在输出",
    );
  });

  it("keeps a growing heading semantic and current", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<StreamingMessageBody content="# 标" />));
    const heading = host.querySelector("h1");
    act(() => root?.render(<StreamingMessageBody content="# 标题继续增长" />));
    expect(host.querySelector("h1")?.textContent).toBe("标题继续增长");
    expect(host.querySelector("h1")).toBe(heading);
  });

  it("keeps CRLF headings and fenced code visible through incremental input", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const first = "# 标题\r\n\r\n```txt\r\n代码";
    act(() => root?.render(<StreamingMessageBody content={first} />));
    const heading = host.querySelector("h1");
    expect(heading?.textContent).toBe("标题");
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={first + "继续\r\n```\r\n"}
          streaming={false}
        />,
      ),
    );
    expect(host.querySelector("h1")).toBe(heading);
    expect(host.querySelector("code")?.textContent).toBe("代码继续");
  });

  it("continues a loose list without duplicating its first item", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<StreamingMessageBody content={"- 第一项\n\n"} />));
    const list = host.querySelector("ul");
    act(() =>
      root?.render(<StreamingMessageBody content={"- 第一项\n\n- 第二项"} />),
    );
    expect(host.querySelectorAll("li")).toHaveLength(2);
    expect(host.querySelector("ul")).toBe(list);
  });

  it("retains stable DOM on completion and has no empty tail", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(<StreamingMessageBody content={"第一段\n\n第二段"} />),
    );
    const first = host.querySelector("p");
    act(() =>
      root?.render(
        <StreamingMessageBody content={"第一段\n\n第二段"} streaming={false} />,
      ),
    );
    expect(host.querySelector("p")).toBe(first);
    expect(host.querySelector("[data-streaming-tail]")).toBeNull();
  });

  it("windows a 200k answer while retaining its end and source offsets", () => {
    host = document.createElement("div");
    host.setAttribute("data-radix-scroll-area-viewport", "");
    document.body.append(host);
    root = createRoot(host);
    const content = Array.from(
      { length: 2500 },
      (_, i) => `段落${i} ${"全文".repeat(40)}\n\n`,
    ).join("");
    act(() => root?.render(<StreamingMessageBody content={content} />));
    expect(content.length).toBeGreaterThan(200_000);
    expect(host.querySelectorAll("[data-markdown-block]").length).toBeLessThan(
      60,
    );
    expect(host.textContent).toContain("段落0");
    expect(host.textContent).toContain("段落2499");
  });

  it("windows giant code and paragraph blocks without omission notices", () => {
    host = document.createElement("div");
    host.setAttribute("data-radix-scroll-area-viewport", "");
    document.body.append(host);
    root = createRoot(host);
    const content =
      `# 完整答案\n\n${"A".repeat(220_000)}\n\n` +
      "```txt\n" +
      "B".repeat(80_000) +
      "\n```";
    act(() =>
      root?.render(
        <StreamingMessageBody content={content} streaming={false} />,
      ),
    );
    expect(host.textContent?.length).toBeLessThan(60_000);
    expect(host.textContent).toContain("完整答案");
    expect(host.textContent).not.toContain("truncated");
  });

  it("resolves a later reference definition without replacing prior paragraph", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(<StreamingMessageBody content={"[文档][ref]\n\n下一段"} />),
    );
    const first = host.querySelector("p");
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={"[文档][ref]\n\n下一段\n\n[ref]: https://example.com\n"}
        />,
      ),
    );
    expect(host.querySelector("a")?.getAttribute("href")).toBe(
      "https://example.com",
    );
    expect(host.querySelector("p")).toBe(first);
  });

  it("retains code and table containers when growing beyond a row window", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<StreamingMessageBody content={"```txt\nabc"} />));
    const pre = host.querySelector("pre");
    act(() =>
      root?.render(
        <StreamingMessageBody content={"```txt\nabc" + "x\n".repeat(5000)} />,
      ),
    );
    expect(host.querySelector("pre")).toBe(pre);
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={"| A | B |\n| - | - |\n| 1 | 2 |"}
          contentIdentity="table"
        />,
      ),
    );
    const table = host.querySelector("table");
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={
            "| A | B |\n| - | - |\n| 1 | 2 |\n" + "| 3 | 4 |\n".repeat(1500)
          }
          contentIdentity="table"
        />,
      ),
    );
    expect(host.querySelector("table")).toBe(table);
    expect(host.querySelectorAll("table")).toHaveLength(1);
  });

  it("preserves inline link semantics across finalized long prose windows", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={`[${"A".repeat(30_000)}](https://example.com)`}
          streaming={false}
        />,
      ),
    );
    const links = host.querySelectorAll("a");
    expect(
      host.querySelector("[data-source-row]")?.getAttribute("data-source-row"),
    ).toBe("0");
    expect(links[0]?.textContent?.startsWith("AAAA")).toBe(true);
    expect(links.length).toBeGreaterThan(0);
    expect(
      [...links].every(
        (link) => link.getAttribute("href") === "https://example.com",
      ),
    ).toBe(true);
    expect(host.textContent).not.toContain("[");
  });

  it("keeps prose nodes across window activation and completion", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(<StreamingMessageBody content={"a".repeat(8000)} />),
    );
    const paragraph = host.querySelector("p");
    act(() =>
      root?.render(<StreamingMessageBody content={"a".repeat(8300)} />),
    );
    expect(host.querySelector("p")).toBe(paragraph);
    act(() =>
      root?.render(
        <StreamingMessageBody content={"a".repeat(8300)} streaming={false} />,
      ),
    );
    expect(host.querySelector("p")).toBe(paragraph);
  });

  it("does not split a long list item at each inline mark", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={"- before **bold** after " + "z".repeat(9000)}
          streaming={false}
        />,
      ),
    );
    const first = host.querySelector("[data-source-row]");
    expect(first?.querySelectorAll("li")).toHaveLength(1);
    expect(first?.querySelector("li")?.textContent).toContain(
      "before bold after",
    );
  });

  it.each([
    { content: `- **${"A".repeat(10_000)}**`, selector: "li strong" },
    {
      content: `[${"A".repeat(10_000)}](https://example.com)`,
      selector: "p a",
    },
    { content: `# ${"A".repeat(10_000)}`, selector: "h1" },
  ])(
    "keeps active long syntax and completed row nodes: $selector",
    ({ content, selector }) => {
      host = document.createElement("div");
      document.body.append(host);
      root = createRoot(host);
      act(() => root?.render(<StreamingMessageBody content={content} />));
      const rows = [...host.querySelectorAll("[data-source-row]")];
      expect(rows.length).toBeGreaterThan(1);
      const first = rows[0]?.querySelector(selector);
      expect(first).not.toBeNull();
      expect(
        rows.every((row) =>
          row.querySelector(selector)?.textContent?.includes("AAAA"),
        ),
      ).toBe(true);
      act(() =>
        root?.render(
          <StreamingMessageBody content={content} streaming={false} />,
        ),
      );
      expect(host.querySelector(selector)).toBe(first);
    },
  );

  it("preserves whitespace at a plain prose row boundary", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={"A".repeat(2048) + "    " + "B".repeat(9000)}
        />,
      ),
    );
    expect(
      host
        .querySelector("[data-source-row='2048'] p")
        ?.textContent?.startsWith("    BBBB"),
    ).toBe(true);
  });

  it("keeps parser input bounded while 200k plain prose grows", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const parse = vi.spyOn(markdownContract, "renderMarkdownWithProfile");
    let content = "普通长段落。".repeat(34_000);
    act(() => root?.render(<StreamingMessageBody content={content} />));
    expect(parse.mock.calls.length).toBeGreaterThan(0);
    expect(parse.mock.calls.every(([source]) => source.length < 2100)).toBe(
      true,
    );
    const first = host.querySelector("p");
    parse.mockClear();
    for (let i = 0; i < 12; i += 1) {
      content += "继续输出。";
      act(() => root?.render(<StreamingMessageBody content={content} />));
    }
    expect(host.querySelector("p")).toBe(first);
    expect(parse.mock.calls.length).toBeLessThanOrEqual(24);
    expect(parse.mock.calls.every(([source]) => source.length < 2100)).toBe(
      true,
    );
    const calls = parse.mock.calls.length;
    act(() =>
      root?.render(
        <StreamingMessageBody content={content} streaming={false} />,
      ),
    );
    expect(parse.mock.calls.length).toBe(calls);
    expect(host.querySelector("p")).toBe(first);
  });

  it("keeps a combined grapheme together across a semantic row cut", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const content = `**${"A".repeat(2047)}e\u0301${"B".repeat(9000)}**`;
    act(() => root?.render(<StreamingMessageBody content={content} />));
    expect(
      host
        .querySelector("[data-source-row] strong")
        ?.textContent?.endsWith("e\u0301"),
    ).toBe(true);
  });

  it("invalidates plain row templates when appended syntax changes earlier text", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const content = "A".repeat(10_000);
    act(() => root?.render(<StreamingMessageBody content={content} />));
    const first = host.querySelector("p");
    act(() =>
      root?.render(<StreamingMessageBody content={content + " **bold**"} />),
    );
    expect(host.querySelector("p")).toBe(first);
    expect(host.querySelector("strong")?.textContent).toBe("bold");
  });

  it("ignores reference definitions before code and table containers", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <StreamingMessageBody
          content={
            "[x]: https://example.com\n\n```js\nhello\n```\n\n| A | B |\n| - | - |\n| 1 | 2 |"
          }
          streaming={false}
        />,
      ),
    );
    expect(host.querySelector("code")?.textContent?.trim()).toBe("hello");
    expect(
      [...host.querySelectorAll("th")].map((node) => node.textContent),
    ).toEqual(["A", "B"]);
    act(() =>
      root?.render(
        <StreamingMessageBody content={"```\n```"} streaming={false} />,
      ),
    );
    expect(host.querySelector("code")?.textContent).toBe("");
  });

  it("publishes the tail line budget after updating visible text", () => {
    const frames: FrameRequestCallback[] = [];
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      frames.push(callback);
      return frames.length;
    });
    const budgetRef: { current: StreamingLineBudget | null } = {
      current: null,
    };
    const descriptor = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "clientWidth",
    );
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get() {
        return 180;
      },
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    try {
      act(() => {
        root?.render(
          <StreamingLineBudgetRefContext.Provider value={budgetRef}>
            <StreamingMessageBody
              content={"仍在输出"}
              contentIdentity="run-budget"
            />
          </StreamingLineBudgetRefContext.Provider>,
        );
      });
      act(() => frames.splice(0).forEach((callback) => callback(16)));
      expect(budgetRef.current?.lineWidthPx).toBe(180);
      expect(budgetRef.current?.remainingPx).toBeLessThanOrEqual(180);
    } finally {
      if (descriptor) {
        Object.defineProperty(HTMLElement.prototype, "clientWidth", descriptor);
      } else {
        Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
      }
    }
  });
});
