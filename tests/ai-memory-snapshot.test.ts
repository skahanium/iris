import { describe, expect, it } from "vitest";

import {
  createAiMemorySnapshot,
  createAiStressPayload,
} from "@/lib/ai-memory-snapshot";
import { createAiPayloadStore, projectTextForUi } from "@/lib/ai-payload-store";

describe("AI memory snapshot", () => {
  it("summarizes lengths and hashes without storing raw content", () => {
    const store = createAiPayloadStore();
    const secret = "secret answer".repeat(20_000);
    projectTextForUi(store, secret, { kind: "assistant_message" });
    const stress = createAiStressPayload(8_000, "secret-marker");

    const snapshot = createAiMemorySnapshot({
      phase: "streaming",
      messages: [
        { role: "user", content: "secret prompt" },
        { role: "assistant", content: "secret answer" },
      ],
      streamLength: 12,
      renderWindowLength: 8,
      markdownCache: { entryCount: 1, estimatedBytes: 128 },
      workerInFlightBytes: 64,
      domTextLength: 32,
      payloadStore: store.snapshot(),
      artifacts: [stress.artifact],
      packets: [stress.evidencePacket],
      taskEvents: [stress.taskEvent],
      docSummaryLength: 10,
      researchResult: { summary: stress.assistantText },
    });

    expect(snapshot).toMatchObject({
      phase: "streaming",
      messageCount: 2,
      maxMessageLength: 13,
      streamLength: 12,
      renderWindowLength: 8,
      markdownCacheEntryCount: 1,
      markdownCacheEstimatedBytes: 128,
      workerInFlightBytes: 64,
      domTextLength: 32,
      payloadStoreEntryCount: 1,
      artifactCount: 1,
      packetCount: 1,
      taskEventCount: 1,
      docSummaryLength: 10,
    });
    expect(snapshot.messageHashes).toHaveLength(2);
    expect(snapshot.artifactEstimatedBytes).toBeGreaterThan(8_000);
    expect(JSON.stringify(snapshot)).not.toContain("secret prompt");
    expect(JSON.stringify(snapshot)).not.toContain("secret answersecret");
    expect(JSON.stringify(snapshot)).not.toContain("secret-marker");
  });
});
