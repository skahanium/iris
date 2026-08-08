//! Pure reasoning/model helpers for the LLM routing section.
//!
//! Extracted from LlmRoutingSection so the component stays a thin orchestrator.

import type {
  LlmConfigGetResponse,
  LlmRoutingConfig,
  ModelCatalogEntry,
  ReasoningControl,
  ReasoningMode,
} from "@/types/llm";
import type { LlmEnabledProviderModel } from "./llmProviderTypes";
import builtinLlmProviders from "../../../config/llm-builtin-providers.json";

type EnabledProviderModel = LlmEnabledProviderModel;

export const FALLBACK_PROVIDERS: LlmConfigGetResponse["providers"] =
  builtinLlmProviders.map((provider) => ({
    id: provider.id,
    name: provider.name,
    default_model: provider.defaultModel,
    endpointManaged: "builtin",
  }));

export const REASONING_STRENGTH_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
];

const DEEPSEEK_REASONING_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "high",
  "xhigh",
];

const OPENAI_REASONING_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "minimal",
  "low",
  "medium",
  "high",
];

export const REASONING_EFFORT_OPTIONS: ReasoningMode[] = [
  "off",
  "auto",
  "low",
  "medium",
  "high",
];

export const REASONING_SWITCH_OPTIONS: ReasoningMode[] = ["off", "on", "auto"];
export const UNSUPPORTED_REASONING_CAPABILITY: ReasoningUiCapability = {
  supported: false,
  control: "none",
  tagOnly: false,
  supportedModes: [],
  defaultMode: "off",
  disableSupported: true,
  source: "unknown",
};

export interface ReasoningUiCapability {
  supported: boolean;
  control: ReasoningControl;
  tagOnly: boolean;
  supportedModes: ReasoningMode[];
  defaultMode: ReasoningMode;
  disableSupported: boolean;
  source: "catalog" | "probe" | "user" | "unknown";
}
export function nextCustomProviderId(existing: Iterable<string>): string {
  const set = new Set(existing);
  if (!set.has("custom")) return "custom";
  let n = 2;
  while (set.has(`custom_${n}`)) n += 1;
  return `custom_${n}`;
}

export function uniqueModelIds(models: Iterable<string>): string[] {
  const out: string[] = [];
  for (const model of models) {
    const trimmed = model.trim();
    if (trimmed && !out.includes(trimmed)) out.push(trimmed);
  }
  return out;
}

export function parseModelIds(input: string): string[] {
  return uniqueModelIds(input.split(/[\n,，]/));
}

export function modelReferenceValue(
  providerId: string,
  modelId: string,
): string {
  return JSON.stringify([providerId, modelId]);
}

export function normalizeCandidateOrder(
  providers: LlmRoutingConfig["providers"],
  candidateOrder: readonly { providerId: string; modelId: string }[],
): { providerId: string; modelId: string }[] {
  const enabled = new Set(
    Object.entries(providers).flatMap(([providerId, provider]) =>
      uniqueModelIds(provider.enabledModels ?? []).map((modelId) =>
        modelReferenceValue(providerId, modelId),
      ),
    ),
  );
  const normalized: { providerId: string; modelId: string }[] = [];
  for (const candidate of candidateOrder) {
    const providerId = candidate.providerId.trim();
    const modelId = candidate.modelId.trim();
    const key = modelReferenceValue(providerId, modelId);
    if (
      providerId &&
      modelId &&
      enabled.has(key) &&
      !normalized.some(
        (item) => modelReferenceValue(item.providerId, item.modelId) === key,
      )
    ) {
      normalized.push({ providerId, modelId });
    }
  }
  for (const key of [...enabled].sort()) {
    if (
      normalized.some(
        (item) => modelReferenceValue(item.providerId, item.modelId) === key,
      )
    ) {
      continue;
    }
    const parsed: unknown = JSON.parse(key);
    if (
      Array.isArray(parsed) &&
      typeof parsed[0] === "string" &&
      typeof parsed[1] === "string"
    ) {
      normalized.push({ providerId: parsed[0], modelId: parsed[1] });
    }
  }
  return normalized;
}

export function findModelCatalogForProvider(
  catalog: ModelCatalogEntry[] | undefined,
  providerId: string,
  modelId: string,
): ModelCatalogEntry | undefined {
  return catalog?.find(
    (model) =>
      model.providerId === providerId &&
      model.id.toLowerCase() === modelId.toLowerCase(),
  );
}

export function textValidatedModel(model: EnabledProviderModel): boolean {
  return Boolean(
    model.catalog ||
    model.registry?.textVerifiedAt ||
    model.registry?.visionVerifiedAt,
  );
}

export function modelSupportsVision(model: EnabledProviderModel): boolean {
  const probeTimestamp = model.registry?.visionVerifiedAt;
  if (probeTimestamp && probeTimestamp !== "built_in") return true;
  if (model.catalog) return model.catalog.supportsVision;
  return Boolean(probeTimestamp);
}

export function modelCapabilitySummary(
  model: EnabledProviderModel,
  result: { ok: boolean; message: string } | undefined,
  reasoningSummary: string,
): string {
  if (result) return result.message;
  const textReady = textValidatedModel(model);
  if (!textReady) return "未验证";
  const visionReady = modelSupportsVision(model);
  // When a live probe confirmed vision but the catalog disagrees,
  // surface the probe result with a clarifying label.
  const probeVision =
    model.registry?.visionVerifiedAt &&
    model.registry.visionVerifiedAt !== "built_in";
  const catalogSaysNo = model.catalog && !model.catalog.supportsVision;
  const visionLabel = visionReady
    ? probeVision && catalogSaysNo
      ? "视觉可用 (探测确认)"
      : "视觉可用"
    : "视觉不支持";
  const base = `文本可用 · ${visionLabel}`;
  return `${base} · ${reasoningSummary}`;
}

export function modelLooksTagReasoningRisk(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return (
    provider.includes("minimax") ||
    /minimax/i.test(modelId) ||
    /^minimax-m3$/i.test(modelId)
  );
}

export function modelLooksOpenAiReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "openai" && /^(o1|o3|o4|gpt-5)/i.test(modelId);
}

export function modelLooksDeepSeekReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "deepseek" || /^deepseek-/i.test(modelId);
}

export function modelLooksGlmReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "zhipu" && /^(glm-4\.5|glm-5)/i.test(modelId);
}

export function modelLooksQwenReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return (
    provider.includes("qwen") ||
    provider.includes("dashscope") ||
    /qwen3/i.test(modelId)
  );
}

export function modelLooksGeminiReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return (
    (provider === "google" || provider === "gemini") &&
    /gemini-2\.5/i.test(modelId)
  );
}

export function modelLooksHunyuanReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "hunyuan" && /hunyuan-t1/i.test(modelId);
}

export function modelLooksErnieReasoning(
  providerId: string,
  modelId: string,
): boolean {
  const provider = providerId.toLowerCase();
  return provider === "ernie" && /ernie-x1/i.test(modelId);
}

export function catalogReasoningCapability(
  providerId: string,
  modelId: string,
  catalog: ModelCatalogEntry | undefined,
): ReasoningUiCapability | null {
  if (modelLooksDeepSeekReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "effort",
      tagOnly: false,
      supportedModes: DEEPSEEK_REASONING_OPTIONS,
      defaultMode: "high",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksOpenAiReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "effort",
      tagOnly: false,
      supportedModes: OPENAI_REASONING_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (catalog?.providerId === "anthropic" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "budget",
      tagOnly: false,
      supportedModes: REASONING_STRENGTH_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksGlmReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "effort",
      tagOnly: false,
      supportedModes: REASONING_EFFORT_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksGeminiReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "level",
      tagOnly: false,
      supportedModes: REASONING_EFFORT_OPTIONS,
      defaultMode: "medium",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksQwenReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "tag",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (catalog?.providerId === "minimax" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "tag",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksHunyuanReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "tag",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (modelLooksErnieReasoning(providerId, modelId)) {
    return {
      supported: true,
      control: "tag",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "auto",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (providerId === "mimo") {
    return {
      supported: true,
      control: "switch",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "on",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (catalog?.providerId === "mimo" && catalog.supportsThinking) {
    return {
      supported: true,
      control: "switch",
      tagOnly: true,
      supportedModes: REASONING_SWITCH_OPTIONS,
      defaultMode: "on",
      disableSupported: true,
      source: "catalog",
    };
  }
  if (catalog) {
    return {
      supported: false,
      control: "none",
      tagOnly: false,
      supportedModes: ["off"],
      defaultMode: "off",
      disableSupported: true,
      source: "catalog",
    };
  }
  return null;
}

export function reasoningOptionsForCapability(
  capability: ReasoningUiCapability,
): ReasoningMode[] {
  if (!capability.supported) return [];
  if (capability.supportedModes.length > 0) return capability.supportedModes;
  if (
    capability.control === "effort" ||
    capability.control === "level" ||
    capability.control === "budget"
  ) {
    return REASONING_STRENGTH_OPTIONS;
  }
  return REASONING_SWITCH_OPTIONS;
}

export const REASONING_SOURCE_LABELS: Record<
  ReasoningUiCapability["source"],
  string
> = {
  catalog: "来源：内置目录",
  probe: "来源：验证探测",
  user: "来源：用户确认",
  unknown: "来源：未知",
};

export function reasoningSourceLabel(
  source: ReasoningUiCapability["source"],
): string {
  return REASONING_SOURCE_LABELS[source];
}

export function reasoningCapabilitySummary(
  capability: ReasoningUiCapability,
): string {
  const source = reasoningSourceLabel(capability.source);
  if (capability.source === "unknown") return `推理未知（${source}）`;
  if (!capability.supported) return `推理不支持（${source}）`;
  const detail =
    capability.control === "effort" ||
    capability.control === "budget" ||
    capability.control === "level"
      ? "支持强度"
      : "无强度控制";
  return `推理可用（${detail}，${source}）`;
}
