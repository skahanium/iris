# Unified Run Policy Design

## Purpose

Make `PolicyDecisionEngine` the single decision point used by the new Agent Run path before Provider, Web, tool, or write dispatch. A Run must use only explicit request facts; active editor state, legacy scene/intent, and implicit current document state are not policy inputs.

## Scope

This design covers the new `assistant_run_*` path only. Legacy commands are not adapted; they will be removed in the later legacy-chain deletion phase. Classified Runs remain CEF-only and initially support only a direct, offline answer.

## Decision model

`RunPolicyRequest` contains the resolved `ExecutionEnvelope`, the explicit reference list, and an explicit security-domain value. `PolicyDecision` returns an allow/deny result, stable safe code, and the complete permitted capability set. The decision is immutable for the dispatch attempt and is checked again immediately before any capability dispatch.

The policy rules are:

- Classified domain denies `web.search`, `web.fetch`, MCP, normal-vault reads, normal conversation memory, and normal evidence storage.
- `Freshness::Offline` denies every Web capability, regardless of UI toggle or a later executor request.
- `Effect::Answer` permits no document write capability.
- `Effect::Draft` permits only `note.propose_patch`; `Effect::Apply` additionally requires an explicit target and `note.apply_patch` remains confirmation-gated.
- Explicit references are individually checked against the resolved document policy. An explicit reference never overrides a deny for `read` or `send_to_model`.
- Provider dispatch requires `model.text`; image input also requires `model.vision`. Missing or denied capabilities return a stable safe denial before any network request.

## Integration boundary

`RunIntake` resolves and persists the envelope first. The Run executor calls the policy service after accepted has been persisted, records a safe `permission_denied` event when the decision is denied, and does not instantiate a Provider, Web adapter, tool, or legacy Harness. Provider routing accepts only the policy-approved capability requirements.

## Verification

Tests must prove that classified Runs never request Web/MCP/normal evidence, offline is non-bypassable, an explicit reference cannot override document deny, and a policy denial happens before Provider construction. Existing normal direct-answer behavior remains one Provider call when allowed.
