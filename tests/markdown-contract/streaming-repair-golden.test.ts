import { describe, expect, it } from "vitest";

import { repairStreamingMarkdown } from "@/lib/markdown-render";

const repairCases = [
  ["image destination", "![alt](image.png", "![alt](image.png)"],
  [
    "link destination",
    "[docs](https://example.com",
    "[docs](https://example.com)",
  ],
  ["obvious table row", "| A | B", "| A | B |"],
  ["bold delimiter", "**partial", "**partial**"],
  ["footnote ref", "See [^note", "See [^note]"],
] as const;

const stableCases = [
  "![alt](image.png)",
  "[docs](https://example.com)",
  "| A | B |",
  "| A |\n| --- |",
  "plain text with | pipe but not a table",
  "[^note]: Complete definition.",
] as const;

describe("streaming markdown repair golden corpus", () => {
  it.each(repairCases)("repairs %s predictably", (_name, input, expected) => {
    expect(repairStreamingMarkdown(input)).toBe(expected);
  });

  it.each(stableCases)("leaves complete markdown unchanged: %s", (input) => {
    expect(repairStreamingMarkdown(input)).toBe(input);
  });

  it.each([...repairCases.map(([, input]) => input), ...stableCases])(
    "is idempotent for %s",
    (input) => {
      const once = repairStreamingMarkdown(input);
      expect(repairStreamingMarkdown(once)).toBe(once);
    },
  );
});
