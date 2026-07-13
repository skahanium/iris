import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI security-domain and Run routing", () => {
  it("derives the active security domain instead of a scene or task-plan route", () => {
    const routing = read("src/hooks/useWorkspaceAssistantRouting.ts");

    expect(routing).toContain(
      'import { deriveAiDomainState } from "@/lib/ai-domain"',
    );
    expect(routing).toContain("deriveAiDomainState({");
    expect(routing).toContain("aiDomain: domainState.domain");
    expect(routing).not.toContain("activeArtifactTab");
    expect(routing).toContain(
      "classifiedPath: domainState.classifiedActivePath",
    );
  });

  it("passes only domain, classified path and mention candidates into the panel slot", () => {
    const slot = read("src/components/layout/AppAiPanelSlot.tsx");

    expect(slot).toContain("aiDomain={aiDomain}");
    expect(slot).toContain("classifiedPath={classifiedPath}");
    expect(slot).toContain(
      "runtimeDocumentCandidates={mentionRuntimeCandidates}",
    );
    expect(slot).not.toContain("notePath=");
    expect(slot).not.toContain("getNoteContent=");
    expect(slot).not.toContain("getLiveMarkdown");
  });

  it("starts the panel through the Run controller with explicit references only", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const sender = read("src/components/ai/hooks/useUnifiedAssistantSend.ts");

    expect(panel).toContain("useAssistantRun()");
    expect(panel).toContain("useUnifiedAssistantSend({");
    expect(sender).toContain("explicitReferences");
    expect(sender).toContain("securityDomain: aiDomain");
    expect(sender).not.toContain("noteContent");
    expect(sender).not.toContain("getNoteContent");
  });

  it("keeps classified and normal conversations on opaque domain-safe session references", () => {
    const types = read("src/types/ai.ts");

    expect(types).toContain("export type SecurityDomain");
    expect(types).toContain("export interface AssistantSessionRef");
    expect(types).toContain("domain: SecurityDomain");
    expect(types).toContain("sessionKey: string");
  });

  it("uses Run event streaming for both panel and inline editor actions", () => {
    const inline = read("src/hooks/useInlineAi.ts");
    const events = read("src/lib/ipc-events.ts");

    expect(inline).toContain("assistantRunStart");
    expect(inline).toContain("listenAssistantRunEvent");
    expect(inline).toContain('event.type === "content_delta"');
    expect(events).toContain('ASSISTANT_RUN_EVENT: "assistant:run_event"');
  });
});
