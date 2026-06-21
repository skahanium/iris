# Agent Harness TaskPlan Blueprint

Date: 2026-06-21

## Summary

Iris will move from keyword scene routing and workflow-owned UI artifacts to a per-turn `TaskPlan` driven agent harness. A single conversation may naturally move between lightweight chat, local note Q&A, creative writing, research, document work, citation checks, and follow-up discussion. Each user turn gets its own task plan; conversation memory carries continuity, but no previous scene may lock the next turn.

The main product rule is strict: the assistant conversation surface is Markdown-first text. It may render normal Iris-supported Markdown such as paragraphs, headings, lists, tables, blockquotes, links, and code blocks. It must not render research cards, process cards, evidence matrices, workspace-open cards, or workflow-specific UI chrome inside the message stream.

Structured work belongs in temporary tabs only when it has clear user value. Temporary tabs are high-value work products, not automatic workflow byproducts. The implementation should aggressively delete obsolete scene/router/artifact code instead of preserving unnecessary compatibility layers.

## Goals

- Correctly distinguish the current turn's intent without pinning the whole session to one scene.
- Keep ordinary requests fast by avoiding unnecessary classifier calls, research decomposition, long-context setup, and artifact generation.
- Make creative writing and document collaboration first-class without routing them through research.
- Unify editor selection, right-side assistant context, and downstream workflows through explicit context references.
- Collapse `search_web` and `fetch_web_page` product semantics into one user-facing network capability.
- Remove low-value temporary tabs and legacy scene-driven behavior as soon as their replacements exist.

## Non-Goals

- Do not build a generic workflow DAG platform.
- Do not add a user-visible scene selector.
- Do not keep old and new harness systems in parallel beyond the minimum migration window.
- Do not make temporary tabs a replacement for readable answers.
- Do not allow AI to modify `.md` content without confirmation.

## Core Decisions

### Per-Turn TaskPlan

Add a task planning layer that produces a compact `TaskPlan` for every assistant turn. The plan should include:

- `intent`: chat, ask_notes, creative_write, rewrite_selection, citation_check, research, organize, document_check, chapter, vision_chat, skill_management, or a closely equivalent existing enum.
- `confidence`: high, medium, or low.
- `context_references`: selected document ranges and other explicit context handles.
- `retrieval_mode`: none, current_reference, local_notes, scoped_notes, or long_document.
- `web_mode`: disabled or brokered.
- `model_slot`: Fast, Writer, Reasoner, LongContext, Vision, or AgentTools.
- `execution_mode`: direct_answer, context_answer, writing_candidate, patch_proposal, structured_task, long_task, or clarification.
- `output_mode`: markdown_message, artifact_backed_message, confirmation_required, or diagnostic.
- `artifact_plan`: zero or more proposed temporary tabs with kind and eligibility reason.
- `requires_clarification`: true only when the system should ask a short natural-language confirmation before starting a costly or ambiguous task.

The task plan is the source of truth for model routing, task policy, tool capability exposure, prompt focus, and artifact generation. Legacy `AiScene` may remain only as a compatibility field for old sessions, traces, and migrations.

### Routing Paths

Use three routing paths:

- Fast Path: deterministic rules handle explicit UI actions, attached images, context references, obvious chat, obvious local note Q&A, and obvious creative writing. This path must not call a classifier model.
- Clarify Path: low-confidence or high-cost ambiguity returns a short assistant question in the normal message stream. It must not silently start research, document checks, or multi-tool workflows.
- Heavy Path: only explicit or confirmed deep research, whole-document checks, multi-source verification, or recoverable long tasks enter Reasoner, LongContext, AgentTools, or multi-round execution.

The existing keyword rule where terms such as "分析", "研究", or "综述" can directly force research is too brittle. In creative or document-writing context, those terms often describe how to write, not a request for a research workflow.

### Model Slot Policy

Model slot selection is derived from `TaskPlan`, not legacy scene:

- Fast for lightweight conversation, simple note Q&A, and quick classification only when classification is unavoidable.
- Writer for creative generation, rewriting, continuation, style work, and writing candidates.
- Reasoner for confirmed deep analysis, research synthesis, or complex multi-step reasoning.
- LongContext for confirmed whole-document tasks.
- Vision for image-attached turns.
- AgentTools for skill management or tool-dense tasks.

This keeps simple requests quick while still allowing heavier models when the task actually needs them.

## Conversation Surface Contract

The assistant message list renders ordinary Markdown text only. It may include lightweight Markdown links or compact textual references to temporary tabs, but it must not embed workflow-specific cards or controls.

Research output should appear as a readable Markdown answer in the conversation. Evidence coverage, source details, conflicts, gaps, and matrix-like views belong in a temporary evidence/source tab.

Writing output should appear as Markdown text by default. It becomes a patch or modification tab only when the user asks to apply, replace, insert, or otherwise bind the output to a document edit.

Process information appears only when it helps the user recover or understand a non-trivial state: paused budget, failed task, resume, waiting for confirmation, or long-running multi-step task. Ordinary completed tasks do not get a process tab.

## Temporary Tab Taxonomy

Temporary tabs are created only when they pass a value gate.

1. Evidence / Source Tab

Created when there are real sources, citations, conflicts, coverage diagnostics, freshness metadata, or evidence gaps worth inspecting. A "matrix" is only a view inside this tab and should not appear when coverage is empty or mechanically inferred.

2. Writing Modification Tab

Created when there are concrete patches, replacement candidates, insert-after-selection candidates, diff previews, or apply/reject decisions.

3. Structured Result Tab

Created for reusable structured outputs such as organize suggestions, document issue lists, citation reports, or batch actions.

4. Task Process Tab

Created only for long tasks, pause/resume, failure recovery, permission waiting, or meaningful diagnostics. It must not show placeholder summaries such as "assistant workflow output summarized by artifact metadata".

## Context Reference System

Introduce `ContextReference` as a shared contract for editor selection, assistant input chips, workflow context, and model prompts.

It should represent precise selections first:

- document path
- content hash
- exact UTF-8 or editor range
- short display excerpt
- optional heading / neighborhood anchors
- reference kind, such as selection, paragraph, heading, note, or artifact
- stale / invalid status after validation

The system must support cross-sentence, cross-paragraph, partial, and irregular selections. It must not normalize everything to a paragraph unless the user explicitly asks for broader context.

Editor interactions:

- A selected range may open a floating AI composer near the document. The generated result can be inserted after the selection or replace the selection.
- Copying or sending a selected range to the right assistant should create a lightweight reference chip instead of pasting the full source text into the composer.
- Right-side assistant turns may use the reference for chat, writing, research, citation checking, or organization.

If the document hash or range no longer matches at execution time, the assistant should ask the user to refresh the reference instead of using stale content silently.

## Network Evidence Broker

The user-facing model is one network capability controlled by the existing web toggle. Internally, replace user-visible `search_web` versus `fetch_web_page` semantics with a `WebEvidenceBroker`.

When web is disabled, no network tool is called. When web is enabled, the broker may:

- search for candidate sources
- fetch HTTPS page text when needed
- dedupe URLs
- rank source quality
- mark freshness
- record failures and fallbacks
- convert results into evidence items

The conversation should not expose search/fetch tool distinctions. Evidence/source tabs may show what sources were searched or fetched when useful.

Risk boundaries remain separate: downloads, login-required pages, non-HTTPS sources, external writes, and high-risk side effects are not ordinary web evidence operations.

## Technical Debt Policy

This redesign must actively reduce technical debt:

- Do not keep the old scene router as a second decision system once `TaskPlan` covers the path.
- Do not preserve workflow-specific message cards after Markdown-first message rendering lands.
- Do not maintain duplicate artifact mappers that encode different tab rules.
- Do not leave fixed "research result card", "process detail", or "evidence matrix" behavior as compatibility defaults.
- Keep legacy `AiScene` only where required for persisted historical data, trace compatibility, or staged migration.
- Prefer deleting obsolete tests and replacing them with TaskPlan behavior tests over rewriting tests to bless old semantics.

Any temporary compatibility must have an explicit removal target in the implementation plan.

## Acceptance Criteria

- A single conversation can move from chat to creative writing to research and back without scene lock-in.
- A novel continuation request with words like "分析" or "研究" does not enter research workflow unless the user asks for actual research.
- Ordinary assistant messages never render research cards, process cards, evidence matrices, or workspace cards in the message stream.
- Research answers render as Markdown text, with evidence/source details available through a value-gated temporary tab.
- Empty or low-value evidence matrices are not generated.
- Process tabs appear for paused, failed, recoverable, waiting-confirmation, or long multi-step tasks, not for ordinary completed tasks.
- Context references preserve precise irregular selections and validate staleness before use.
- The network toggle controls all ordinary network evidence behavior through one brokered capability.
- Fast-path chat and obvious writing do not perform unnecessary model classification, research decomposition, or long task setup.
- `.md` writes continue to require patch/confirmation.

## Test Strategy

- Unit tests for task plan generation across chat, local Q&A, creative continuation, research, document check, image, and skill-management turns.
- Regression tests for creative writing prompts containing research-like keywords.
- Component tests proving message list renders only Markdown messages for research and document outputs.
- Artifact tests for value gates: evidence/source, writing modification, structured result, and process tabs.
- Context reference tests for partial, cross-paragraph, stale, and hash-mismatch selections.
- Network broker tests for disabled web, enabled search+fetch fusion, dedupe, failure reporting, and evidence conversion.
- Safety tests proving patch application and markdown writes still require confirmation.
- Contract tests ensuring new task policy/model slot routing does not depend on old scene as the primary decision source.
