import type { AiScene } from "@/types/ai";
import type { CapabilitySlot } from "@/types/ai";

export type ContextStrategy = "hybrid" | "long_context";
export type EndpointFamily =
  | "open_ai_compatible_chat_completions"
  | "anthropic_messages"
  | "ollama_chat"
  | "responses_reserved";

export type ProbeStrategy =
  | "open_ai_models_then_chat"
  | "anthropic_messages_ping"
  | "ollama_tags_then_chat"
  | "static_only";

export interface ProviderOverride {
  baseUrl: string | null;
  label?: string | null;
  defaultModel?: string | null;
  enabledModels?: string[] | null;
}

/** 设置页允许的自定义 OpenAI 兼容端点 ID（`custom` 或 `custom_*`）。 */
export function isCustomProviderId(providerId: string): boolean {
  return providerId === "custom" || providerId.startsWith("custom_");
}

export interface SceneRoute {
  providerId: string;
  model: string;
  thinking?: boolean;
}

export type SlotRoute = SceneRoute;

export interface LlmRoutingConfig {
  version: number;
  schemaVersion?: number;
  providers: Record<string, ProviderOverride>;
  slots: Record<CapabilitySlot, SlotRoute>;
  scenes: Record<string, SceneRoute>;
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
  providers: { id: string; name: string; default_model: string }[];
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
    scene: AiScene;
    message: string;
  };
  searchApi: {
    minimaxConfigured: boolean;
    effectiveBackend: "minimax" | "duckduckgo";
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

export const AI_SCENES: AiScene[] = [
  "knowledge_lookup",
  "exemplar_learning",
  "drafting_assist",
  "research_synthesis",
];

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
] satisfies CapabilitySlot[];

/** 客户端回退默认（IPC 不可用或解析失败时） */
export const DEFAULT_LLM_ROUTING: LlmRoutingConfig = {
  version: 1,
  schemaVersion: 2,
  providers: {},
  slots: {
    fast: {
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      thinking: false,
    },
    writer: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: false,
    },
    reasoner: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: true,
    },
    long_context: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: false,
    },
    vision: {
      providerId: "mimo",
      model: "mimo-v2.5",
      thinking: false,
    },
    agent_tools: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: true,
    },
    embedding: {
      providerId: "ollama",
      model: "llama3.2",
      thinking: false,
    },
    reranker: {
      providerId: "ollama",
      model: "llama3.2",
      thinking: false,
    },
    local_private: {
      providerId: "ollama",
      model: "llama3.2",
      thinking: false,
    },
  },
  scenes: {
    knowledge_lookup: {
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      thinking: false,
    },
    exemplar_learning: {
      providerId: "deepseek",
      model: "deepseek-v4-flash",
      thinking: false,
    },
    drafting_assist: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: false,
    },
    research_synthesis: {
      providerId: "deepseek",
      model: "deepseek-v4-pro",
      thinking: false,
    },
  },
  contextStrategy: {
    knowledge_lookup: "hybrid",
    exemplar_learning: "long_context",
    drafting_assist: "long_context",
    research_synthesis: "hybrid",
  },
};
