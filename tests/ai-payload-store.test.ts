import { describe, expect, it } from "vitest";

import {
  createAiPayloadStore,
  projectTextForUi,
  restoreProjectedText,
  sanitizePayloadForUi,
} from "@/lib/ai-payload-store";

describe("AI payload store", () => {
  it("stores long text once and returns a bounded UI projection", () => {
    const store = createAiPayloadStore();
    const fullText = `intro\n${"A".repeat(140_000)}\noutro`;

    const projection = projectTextForUi(store, fullText, {
      kind: "assistant_message",
      maxPreviewChars: 20_000,
    });

    expect(projection.content.length).toBeLessThan(25_000);
    expect(projection.content).toContain("intro");
    expect(projection.content).toContain("outro");
    expect(projection.payloadRef?.length).toBe(fullText.length);
    expect(JSON.stringify(projection)).not.toContain("A".repeat(60_000));
    expect(restoreProjectedText(store, projection)).toBe(fullText);
    expect(store.snapshot()).toMatchObject({ entryCount: 1 });
  });

  it("sanitizes nested task payloads without retaining raw long strings", () => {
    const store = createAiPayloadStore();
    const huge = `secret-start-${"B".repeat(180_000)}-secret-end`;

    const sanitized = sanitizePayloadForUi(
      store,
      {
        task: { status: "paused_budget", user_goal_summary: huge },
        events: [{ event_type: "progress", message: huge }],
        small: "visible summary",
      },
      { maxPreviewChars: 8_000 },
    );

    const serialized = JSON.stringify(sanitized);
    expect(serialized.length).toBeLessThan(30_000);
    expect(serialized).not.toContain("B".repeat(50_000));
    expect(serialized).toContain("contentRef");
    expect(store.snapshot().entryCount).toBe(1);
  });
});
