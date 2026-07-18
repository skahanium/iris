/** AnySearch HTTPS MCP host used by the official preset. */
export const ANYSEARCH_MCP_HOST = "api.anysearch.com";

/** Tool argument name AnySearch expects for result-count limits. */
export const ANYSEARCH_MAX_RESULTS_ARG = "max_results";

export function isAnySearchHost(hostname: string): boolean {
  return hostname.trim().toLowerCase() === ANYSEARCH_MCP_HOST;
}

export function isAnySearchTransportUrl(url: string): boolean {
  try {
    return isAnySearchHost(new URL(url).hostname);
  } catch {
    return false;
  }
}

export function mappingHasMaxResultsArgument(
  raw: string | null | undefined,
): boolean {
  if (!raw?.trim()) return false;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return false;
    }
    return (
      typeof (parsed as Record<string, unknown>).maxResultsArg === "string"
    );
  } catch {
    return false;
  }
}

/**
 * Persist AnySearch search mappings with `maxResultsArg: "max_results"`.
 * Returns the original string when no change is needed.
 */
export function ensureAnySearchSearchMapping(
  mappingJson: string | null | undefined,
): string | null {
  if (mappingJson == null) return null;
  const trimmed = mappingJson.trim();
  if (!trimmed) return null;

  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return trimmed;
    }
    const record = { ...(parsed as Record<string, unknown>) };
    const tool =
      typeof record.tool === "string"
        ? record.tool
        : typeof record.tool_name === "string"
          ? record.tool_name
          : "";
    if (!tool.trim().toLowerCase().includes("search")) {
      return trimmed;
    }
    if (
      typeof record.maxResultsArg === "string" &&
      record.maxResultsArg.trim()
    ) {
      return trimmed;
    }
    record.maxResultsArg = ANYSEARCH_MAX_RESULTS_ARG;
    return JSON.stringify(record);
  } catch {
    return trimmed;
  }
}
