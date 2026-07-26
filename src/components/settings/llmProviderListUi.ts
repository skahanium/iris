import {
  Bot,
  Brain,
  Cpu,
  Globe,
  Settings2,
  Sparkles,
  Waves,
  type LucideIcon,
} from "lucide-react";

import type { LlmEnabledProviderModel } from "./llmProviderTypes";

const PROVIDER_ICON_BY_ID: Record<string, LucideIcon> = {
  openai: Sparkles,
  anthropic: Brain,
  deepseek: Waves,
  google: Globe,
  mimo: Bot,
  minimax: Cpu,
  custom: Settings2,
};

/** Lucide icon for a built-in or custom LLM provider id (no brand logos). */
export function providerIcon(providerId: string): LucideIcon {
  const normalized = providerId.toLowerCase();
  if (PROVIDER_ICON_BY_ID[normalized]) {
    return PROVIDER_ICON_BY_ID[normalized]!;
  }
  if (normalized.startsWith("custom")) {
    return Settings2;
  }
  return Bot;
}

/** True only when Iris vision probe succeeded (registry timestamp, not catalog built_in). */
export function llmModelShowsVisionBadge(
  model: LlmEnabledProviderModel,
): boolean {
  const verifiedAt = model.registry?.visionVerifiedAt;
  return Boolean(verifiedAt && verifiedAt !== "built_in");
}

export const LLM_PROVIDER_LIST_CARD_CLASS =
  "flex w-full items-center gap-3 rounded-lg border border-border/65 bg-background/55 p-3 text-left transition-colors hover:bg-muted/30";
