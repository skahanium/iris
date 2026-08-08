import type { ModelCatalogEntry, ModelRegistryEntry } from "@/types/llm";

export interface LlmVisibleProvider {
  id: string;
  name: string;
  enabledModels: string[];
  configured: boolean;
  custom: boolean;
  endpointManaged: "builtin" | "custom";
  requiresApiKey: boolean;
}

export interface LlmEnabledProviderModel {
  id: string;
  catalog: ModelCatalogEntry | undefined;
  registry: ModelRegistryEntry | undefined;
}
