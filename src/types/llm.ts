import type { AiScene } from "@/types/ai";

export type ContextStrategy = "hybrid" | "long_context";

export interface ProviderOverride {
  baseUrl: string | null;
  label?: string | null;
  defaultModel?: string | null;
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

export interface LlmRoutingConfig {
  version: number;
  providers: Record<string, ProviderOverride>;
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
  cacheFriendly: boolean;
}

export interface LlmConfigGetResponse {
  routing: LlmRoutingConfig;
  providers: { id: string; name: string; default_model: string }[];
  catalog: ModelCatalogEntry[];
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

/** 客户端回退默认（IPC 不可用或解析失败时） */
export const DEFAULT_LLM_ROUTING: LlmRoutingConfig = {
  version: 1,
  providers: {},
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
