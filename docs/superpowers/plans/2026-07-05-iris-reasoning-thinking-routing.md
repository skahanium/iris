# Iris Reasoning / Thinking Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mature Reasoning / Thinking configuration to Iris LLM capability slots, with provider-specific compatibility and robust output isolation.

**Architecture:** Keep `settings.llm_routing` as the route source, upgrade slot reasoning from a boolean to a mode, resolve provider/model capability through catalog + registry + provider overrides, and make gateway adapters generate provider-specific request bodies. Harness treats reasoning as internal by default and only persists sanitized visible answers.

**Tech Stack:** Tauri 2.x, Rust, SQLite existing settings/model registry, React 19, TypeScript, TailwindCSS + shadcn/ui, Vitest, Cargo tests.

---

## File Structure

- Modify `src/types/llm.ts`: add `ReasoningMode`, `ReasoningAdapter`, `ReasoningControl`, model capability override types, and `SlotRoute.reasoning`.
- Modify `src-tauri/src/llm/config.rs`: migrate/normalize slot reasoning, resolve `ResolvedReasoningRequest`, keep old `thinking` compatibility.
- Modify `src-tauri/src/llm/model_catalog.rs`: replace `supports_thinking: bool` usage with structured reasoning capability while preserving serialized `supportsThinking`.
- Modify `src-tauri/src/llm/model_registry.rs`: expose helper functions that merge registry entries with routing `modelCapabilities`; do not add a table.
- Modify `src-tauri/src/commands/llm_config_commands.rs`: include reasoning probe/override data in `llm_config_get`, and extend model validation to record safe probe results in existing routing JSON.
- Modify `src-tauri/src/ai_runtime/model_gateway/body.rs` and streaming modules: generate provider-specific reasoning request fields and consume hidden reasoning channels.
- Modify `src-tauri/src/ai_harness/harness_support.rs` and `src-tauri/src/ai_harness/harness/run.rs`: improve tag extraction, meta-leak cleanup, stream gating, and session persistence sanitization.
- Modify `src-tauri/src/ai_runtime/persona_resolver.rs`: add concise final-answer leakage constraints without freezing Iris style.
- Modify `src/components/settings/LlmRoutingSection.tsx`: add non-Vision reasoning mode dropdown and model capability override controls.
- Modify frontend contract tests under `tests/`; add Rust unit tests in touched Rust modules.
- Update `docs/llm-routing.md` after implementation.

## Task 1: Add Failing Contracts For Slot Reasoning

- [ ] Add Rust tests in `src-tauri/src/llm/config.rs`:
  - old `SlotRoute { thinking: true }` resolves to `ReasoningMode::Auto`;
  - old `thinking: false` resolves to `Off`;
  - non-reasoning model with `mode=high` resolves to no provider reasoning params and output isolation only if tag-risk;
  - Vision slot ignores `reasoning`.

- [ ] Add frontend tests in `tests/llm-reasoning-routing.test.ts`:
  - `LlmRoutingSection.tsx` contains a non-Vision `思考模式` selector;
  - Vision row does not render the selector;
  - disabled label appears for unsupported models;
  - supported models expose `off/auto/low/medium/high`.

- [ ] Run expected failing checks:

```bash
cargo test --lib llm::config::tests::reasoning -- --nocapture
npm run test -- tests/llm-reasoning-routing.test.ts
```

Expected: tests fail because the new types and UI do not exist.

## Task 2: Introduce Reasoning Types And Config Migration

- [ ] In `src/types/llm.ts`, add:

```ts
export type ReasoningMode = "off" | "auto" | "low" | "medium" | "high";
export type ReasoningAdapter =
  | "none"
  | "openai_responses"
  | "anthropic_extended_thinking"
  | "gemini_thinking_config"
  | "deepseek_reasoning_content"
  | "glm_thinking"
  | "qwen_chat_template"
  | "openai_compatible_tag_stream"
  | "provider_specific_static";
export type ReasoningControl = "none" | "switch" | "effort" | "budget";
export type ReasoningVisibility =
  | "hidden_channel"
  | "content_tag"
  | "plain_content_risk";

export interface ReasoningSlotConfig {
  mode: ReasoningMode;
}

export interface ModelCapabilityOverride {
  reasoningAdapter?: ReasoningAdapter;
  reasoningControl?: ReasoningControl;
  reasoningVisibility?: ReasoningVisibility;
  userVerifiedAt?: string | null;
  probeVerifiedAt?: string | null;
}
```

- [ ] Extend `ProviderOverride` with `modelCapabilities?: Record<string, ModelCapabilityOverride> | null`.

- [ ] Extend `SceneRoute` / `SlotRoute` with `reasoning?: ReasoningSlotConfig`.

- [ ] In `normalizeRouting`, map old `thinking` into `reasoning` and preserve any existing `reasoning`.

- [ ] In Rust, add equivalent enums and structs in `llm/config.rs`, using serde `snake_case` for adapter/control/visibility and `camelCase` for slot config fields.

- [ ] Update `LlmRoutingConfig::CURRENT_SCHEMA_VERSION` to `4`; migration only normalizes JSON settings, no SQLite migration.

- [ ] Run:

```bash
cargo test --lib llm::config::tests -- --nocapture
npm run typecheck
```

Expected: config tests and typecheck pass; this task only adds compatible type/schema support.

## Task 3: Build Capability Resolution And Provider Matrix

- [ ] Replace `supports_thinking: bool` decision logic with a resolver that returns `ResolvedReasoningCapability`.

- [ ] Preserve `ModelCatalogEntry.supports_thinking` serialization for existing UI compatibility, but compute it from structured capability.

- [ ] Seed catalog/provider defaults:
  - OpenAI reasoning models: `openai_responses`, `effort`, `hidden_channel`.
  - Anthropic extended thinking capable models: `anthropic_extended_thinking`, `budget`, `hidden_channel`.
  - Gemini thinking capable models: `gemini_thinking_config`, `budget`, `hidden_channel`.
  - DeepSeek reasoner: `deepseek_reasoning_content`, `switch`, `hidden_channel`.
  - GLM 4.5+/5.x: `glm_thinking`, `effort`, `hidden_channel`.
  - Qwen3 hybrid/thinking: `qwen_chat_template`, `switch`, `content_tag`.
  - Doubao / Volc Ark: default `none` unless override/probe enables tag stream.
  - MiniMax / MiniMax-M3: default `openai_compatible_tag_stream`, `none`, `plain_content_risk`.
  - MiMo: `provider_specific_static`, `switch`, `content_tag`.
  - custom OpenAI-compatible: default `none`, then probe/override.

- [ ] Implement merge priority: user override > probe override > catalog > provider/base URL hint > safe default.

- [ ] Add Rust tests for GLM, Qwen, MiniMax, custom unknown, and unsupported model resolution.

- [ ] Run:

```bash
cargo test --lib llm::config::tests llm::model_catalog::tests -- --nocapture
```

Expected: all reasoning capability resolution tests pass.

## Task 4: Add Settings UI Controls

- [ ] In `LlmRoutingSection.tsx`, add helpers:
  - `reasoningOptionsForModel(slot, providerId, modelId)`;
  - `reasoningLabelForCapability(capability)`;
  - `updateSlot(slot, { reasoning: { mode } })`.

- [ ] Render a third select in Fast / Writer / Reasoner / Long context rows.

- [ ] Do not render the control for Vision.

- [ ] When selected model changes, clamp stored mode:
  - unsupported -> `off`;
  - switch-only -> `off` or `auto`;
  - effort/budget -> keep selected mode if valid.

- [ ] Add a compact per-model capability override in the model details area:
  - `自动识别`;
  - `不支持思考`;
  - `原生思考`;
  - `reasoning_content`;
  - `标签隔离`.

- [ ] Run:

```bash
npm run test -- tests/llm-reasoning-routing.test.ts
npm run typecheck
```

Expected: frontend tests and typecheck pass.

## Task 5: Implement Gateway Reasoning Adapters

- [ ] Change `GatewayRequest` from `thinking: bool` to `reasoning: ResolvedReasoningRequest`; keep a constructor/helper for old callers during migration.

- [ ] Implement adapter body generation:
  - `None`: no fields.
  - `OpenAiResponses`: only for supported endpoint/model; otherwise return an internal config error before sending.
  - `AnthropicExtendedThinking`: add extended thinking budget and validate budget < max output.
  - `GeminiThinkingConfig`: generate Gemini body only on Gemini endpoint family.
  - `DeepSeekReasoningContent`: no generic thinking field.
  - `GlmThinking`: add `thinking` object and `reasoning_effort`; map `off=none`, explicit strengths to their matching efforts, and `auto` through the slot default resolver instead of always using the highest effort.
  - `QwenChatTemplate`: inject `/think` or `/no_think` control in the final user/system control layer, not in user-authored note content.
  - `OpenAiCompatibleTagStream`: no request fields.
  - `ProviderSpecificStatic`: only for built-in provider implementations with tests.

- [ ] Extend streaming and non-streaming parsing to collect `reasoning_content` without visible emission.

- [ ] Add tests for each adapter, especially “unsupported model never receives thinking field”.

- [ ] Run:

```bash
cargo test --lib ai_runtime::model_gateway::tests -- --nocapture
```

Expected: provider body and streaming parsing tests pass.

## Task 6: Harden Harness Output Isolation

- [ ] Update `extract_thinking_blocks` to handle:
  - `<think>...</think>`;
  - `<thinking>...</thinking>`;
  - `<reasoning>...</reasoning>`;
  - uppercase/mixed-case tags;
  - unclosed opening tags.

- [ ] Add `sanitize_meta_analysis_prefix` for visible answers. It must remove opening paragraphs that describe user intent, task focus, persona, or “I should...” decisions, while preserving normal answers.

- [ ] In final streaming:
  - `hidden_channel` can stream visible content directly;
  - `content_tag` and `plain_content_risk` first collect internal candidate, sanitize, then emit visible answer.

- [ ] Ensure `session_messages` persists sanitized content only.

- [ ] Add regression tests:
  - MiniMax greeting with English meta-analysis returns only the Chinese greeting;
  - Qwen `<think>` blocks are stripped;
  - unclosed `<think>` does not leak;
  - normal Chinese paragraph beginning with “我觉得...” is preserved.

- [ ] Run:

```bash
cargo test --lib ai_harness::harness_support::tests ai_harness::harness::tests -- --nocapture
```

Expected: all isolation tests pass.

## Task 7: Extend Model Validation Probe Safely

- [ ] Reuse `llm_model_validate`; after successful text validation, run an optional short probe with non-sensitive prompt.

- [ ] Probe order:
  - known catalog model: no speculative native-param probe;
  - custom model: observe `reasoning_content` and `<think>` behavior without extra params;
  - if user selected a native adapter override, try that adapter with a tiny prompt and record success/failure.

- [ ] Store probe results in `settings.llm_routing.providers[providerId].modelCapabilities[modelId]`.

- [ ] If reasoning probe fails but text validation passed, keep model text-usable and show “思考能力未确认”.

- [ ] Add tests:
  - text pass + reasoning fail still marks `textVerifiedAt`;
  - probe result does not store prompt body;
  - custom provider with tag output becomes `openai_compatible_tag_stream`.

- [ ] Run:

```bash
cargo test --lib commands::llm_config_commands::tests llm::model_registry::tests -- --nocapture
```

Expected: validation and persistence behavior pass.

## Task 8: Prompt Guardrails And Diagnostics

- [ ] In `persona_resolver`, add concise final-answer constraints:
  - no internal analysis;
  - no task strategy narration;
  - no persona/config explanation;
  - no tool-choice rationale unless user asks.

- [ ] Keep Iris tone instructions flexible and warm; do not add fixed greeting or answer templates.

- [ ] Emit safe diagnostics in task events:
  - provider;
  - model;
  - capability slot;
  - reasoning mode;
  - adapter;
  - output isolation mode;
  - estimated reasoning/output token budget.

- [ ] Assert diagnostics do not include API keys, prompts, note text, web text, or raw reasoning.

- [ ] Run:

```bash
cargo test --lib ai_runtime::persona_resolver::tests commands::ai_commands::tests -- --nocapture
```

Expected: prompt and diagnostics tests pass.

## Task 9: Documentation And Full Verification

- [ ] Update `docs/llm-routing.md` with:
  - reasoning mode UI behavior;
  - provider compatibility matrix;
  - safe defaults for custom OpenAI-compatible models;
  - privacy rule: raw reasoning is not persisted as ordinary history.

- [ ] Run formatting and checks:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --lib -- --nocapture
npm run lint
npm run format:check
npm run typecheck
npm run test -- tests/llm-reasoning-routing.test.ts tests/assistant-streaming-contract.test.ts tests/assistant-stream-reset.test.tsx
git diff --check
```

Expected: all commands pass.

- [ ] Manual smoke tests:
  - MiniMax-M3 + Fast + `auto`: “你好?” returns a normal greeting, no internal English analysis.
  - GLM 5.x + Reasoner + `high`: request body includes GLM thinking effort.
  - Qwen3 + Reasoner + `auto`: `<think>` is not visible or saved.
  - unsupported model + `high`: UI clamps/blocks and gateway sends no thinking field.
  - Vision slot still works with no reasoning control.

## Implementation Notes

- Do not create a worktree unless the user explicitly approves.
- Do not add database tables or Tauri commands.
- Do not log raw reasoning, prompt bodies, note contents, web contents, or API keys.
- Keep unrelated RAG and provider-status fixes intact; do not revert existing dirty files.
