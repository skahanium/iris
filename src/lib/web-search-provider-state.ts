export interface WebSearchProviderOption {
  id: string;
  name: string;
  providerKind: string;
  enabled: boolean;
  hasSearchMapping: boolean;
}

export type WebSearchAvailabilityReason =
  | "ready"
  | "missing_provider"
  | "provider_unavailable";

export interface WebSearchAvailability {
  canEnable: boolean;
  reason: WebSearchAvailabilityReason;
  detail: string;
  selectedProviderId: string | null;
  effectiveProvider: WebSearchProviderOption | null;
  options: WebSearchProviderOption[];
}

function normalizeSelectedProviderId(
  value: string | null | undefined,
): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : null;
}

function isEnabledMcpSearchProvider(
  provider: WebSearchProviderOption,
): boolean {
  return (
    provider.providerKind === "mcp" &&
    provider.enabled === true &&
    provider.hasSearchMapping === true
  );
}

export function getWebSearchAvailability(
  providers: WebSearchProviderOption[],
  selectedProviderId: string | null | undefined,
): WebSearchAvailability {
  const options = providers.filter(isEnabledMcpSearchProvider);
  const selectedId = normalizeSelectedProviderId(selectedProviderId);
  const selectedProvider = selectedId
    ? (options.find((provider) => provider.id === selectedId) ?? null)
    : null;

  if (options.length === 0) {
    return {
      canEnable: false,
      reason: "missing_provider",
      detail: "未配置可用 MCP 搜索提供方",
      selectedProviderId: selectedId,
      effectiveProvider: null,
      options,
    };
  }

  if (selectedId && !selectedProvider) {
    // Stale selection: fall through to auto-pick among enabled options.
  } else if (selectedProvider) {
    return {
      canEnable: true,
      reason: "ready",
      detail: selectedProvider.name,
      selectedProviderId: selectedId,
      effectiveProvider: selectedProvider,
      options,
    };
  }

  // No explicit usable selection: auto-pick the first enabled MCP search provider.
  return {
    canEnable: true,
    reason: "ready",
    detail: options[0]!.name,
    selectedProviderId: selectedId,
    effectiveProvider: options[0]!,
    options,
  };
}

export function webSearchStatusDetail(
  enabled: boolean,
  availability: WebSearchAvailability,
): string {
  if (!enabled) return "未开启";
  if (!availability.canEnable) return availability.detail;
  const providerName = availability.effectiveProvider?.name.trim();
  return providerName ? `已开启 · ${providerName}` : "已开启";
}
