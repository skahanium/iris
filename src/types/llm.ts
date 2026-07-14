import type { CapabilitySlot } from "@/types/ai";

export type ContextStrategy = "hybrid" | "long_context";
export type EndpointFamily =
  | "open_ai_compatible_chat_completions"
  | "anthropic_messages"
  | "responses_reserved";

export type ProbeStrategy =
  | "open_ai_models_then_chat"
  | "anthropic_messages_ping"
  | "static_only";

export type ReasoningMode =
  | "off"
  | "on"
  | "auto"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh";
export type ReasoningAdapter =
  | "none"
  | "open_ai_responses"
  | "anthropic_extended_thinking"
  | "gemini_thinking_config"
  | "deep_seek_reasoning_content"
  | "glm_thinking"
  | "qwen_chat_template"
  | "open_ai_compatible_tag_stream"
  | "provider_specific_static";
export type ReasoningControl =
  | "none"
  | "switch"
  | "effort"
  | "level"
  | "budget"
  | "tag";
export type ReasoningVisibility =
  | "hidden_channel"
  | "content_tag"
  | "plain_content_risk";

export interface ReasoningSlotConfig {
  mode: ReasoningMode;
}

export interface ModelCapabilityOverride {
  reasoningAdapter?: ReasoningAdapter | null;
  reasoningControl?: ReasoningControl | null;
  reasoningVisibility?: ReasoningVisibility | null;
  supportedModes?: ReasoningMode[];
  defaultMode?: ReasoningMode | null;
  disableSupported?: boolean | null;
  userVerifiedAt?: string | null;
  probeVerifiedAt?: string | null;
}

export interface ProviderOverride {
  baseUrl: string | null;
  label?: string | null;
  defaultModel?: string | null;
  enabledModels?: string[] | null;
  modelCapabilities?: Record<string, ModelCapabilityOverride>;
}

/** 鐠佸墽鐤嗘い闈涘帒鐠佸摜娈戦懛顏勭暰娑?OpenAI 閸忕厧顔愮粩顖滃仯 ID閿涘潉custom` 閹?`custom_*`閿涘鈧?*/
export function isCustomProviderId(providerId: string): boolean {
  return providerId === "custom" || providerId.startsWith("custom_");
}

export interface SceneRoute {
  providerId: string;
  model: string;
  thinking?: boolean;
  reasoning?: ReasoningSlotConfig | null;
}

export type SlotRoute = SceneRoute;

export interface LlmRoutingConfig {
  version: number;
  schemaVersion?: number;
  providers: Record<string, ProviderOverride>;
  slots: Partial<Record<CapabilitySlot, SlotRoute>>;
  contextStrategy: Record<string, ContextStrategy>;
}

export interface ModelCatalogEntry {
  id: string;
  providerId: string;
  displayName: string;
  contextWindow: number;
  maxOutput: number;
  supportsTools: boolean;
  supportsThinking: boolean;
  supportsVision: boolean;
  supportsStreaming: boolean;
  cacheFriendly: boolean;
  endpointFamily: EndpointFamily;
  probeStrategy: ProbeStrategy;
}

export type ModelRegistrySource = "built_in" | "provider_discovered" | "manual";

export type ModelValidationKind = "text" | "vision";

export interface ModelRegistryEntry {
  providerId: string;
  modelId: string;
  displayName: string;
  source: ModelRegistrySource;
  stale: boolean;
  firstSeenAt: string | null;
  lastSeenAt: string | null;
  lastRefreshedAt: string | null;
  textVerifiedAt: string | null;
  visionVerifiedAt: string | null;
  userConfirmedCapabilities: CapabilitySlot[];
}

export interface LlmModelRegistryRefreshResult {
  providerId: string;
  modelCount: number;
  message: string;
}

export interface ModelCapabilityConfirmRequest {
  providerId: string;
  modelId: string;
  slot: CapabilitySlot;
}

export interface LlmConfigGetResponse {
  routing: LlmRoutingConfig;
  providers: {
    id: string;
    name: string;
    default_model: string;
    endpointManaged: "builtin" | "custom";
  }[];
  catalog: ModelCatalogEntry[];
  registry: ModelRegistryEntry[];
}

export type LlmConnectivityState =
  | "ready"
  | "missing_key"
  | "misconfigured"
  | "error";

export interface ConnectivityStatus {
  llm: {
    state: LlmConnectivityState;
    providerId: string;
    model: string;
    message: string;
  };
  searchProvider: {
    configured: boolean;
    providerId?: string | null;
  };
  usageLast?: {
    promptCacheHitTokens: number;
    promptCacheMissTokens: number;
    updatedAt: string;
  };
}

export interface LlmConfigTestResult {
  ok: boolean;
  message: string;
}

export const CAPABILITY_SLOTS: CapabilitySlot[] = [
  "fast",
  "writer",
  "reasoner",
  "long_context",
  "vision",
  "agent_tools",
  "embedding",
  "reranker",
  "local_private",
];

export const USER_CONFIGURABLE_CAPABILITY_SLOTS = [
  "fast",
  "writer",
  "reasoner",
  "long_context",
  "vision",
  "agent_tools",
] satisfies CapabilitySlot[];

/** 鐎广垺鍩涚粩顖氭礀闁偓姒涙顓婚敍鍦汸C 娑撳秴褰查悽銊﹀灗鐟欙絾鐎芥径杈Е閺冭绱?*/
export const DEFAULT_LLM_ROUTING: LlmRoutingConfig = {
  version: 1,
  schemaVersion: 4,
  providers: {},
  slots: {},
  contextStrategy: {},
};
