import { describe, expect, it } from "vitest";

import {
  triageWebCapabilityDegradation,
  WEB_CAPABILITY_DEGRADATION_DOMAIN_LABEL,
} from "@/lib/web-capability-degradation-triage";

describe("triageWebCapabilityDegradation", () => {
  it("maps timeout to MCP domain", () => {
    const triage = triageWebCapabilityDegradation(
      "agent_run_web_provider_timeout",
    );
    expect(triage.domain).toBe("mcp");
    expect(WEB_CAPABILITY_DEGRADATION_DOMAIN_LABEL.mcp).toContain("MCP");
  });

  it("maps evidence required to harness domain", () => {
    const triage = triageWebCapabilityDegradation(
      "agent_run_web_evidence_required",
    );
    expect(triage.domain).toBe("harness");
  });

  it("returns unknown triage for unmapped codes", () => {
    const triage = triageWebCapabilityDegradation("agent_run_cancelled");
    expect(triage.domain).toBe("unknown");
    expect(triage.nextStep).toContain("diagnose-web-capability-degradation");
  });
});
