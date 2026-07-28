import { ChevronRight, Eye } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import {
  LLM_PROVIDER_LIST_CARD_CLASS,
  llmModelShowsVisionBadge,
  providerIcon,
} from "./llmProviderListUi";
import type { LlmEnabledProviderModel } from "./llmProviderTypes";

const MAX_VISIBLE_MODEL_CHIPS = 3;

interface LlmProviderListCardProps {
  providerId: string;
  providerName: string;
  providerModels: LlmEnabledProviderModel[];
  keyConfigured: boolean;
  onSelect: () => void;
}

export function LlmProviderListCard({
  providerId,
  providerName,
  providerModels,
  keyConfigured,
  onSelect,
}: LlmProviderListCardProps) {
  const Icon = providerIcon(providerId);
  const visibleModels = providerModels.slice(0, MAX_VISIBLE_MODEL_CHIPS);
  const hiddenModelCount = providerModels.length - visibleModels.length;

  return (
    <Button
      asChild
      variant="ghost"
      size="sm"
      className="h-auto w-full p-0 font-normal hover:bg-transparent"
    >
      <button
        type="button"
        data-testid="llm-provider-card"
        className={LLM_PROVIDER_LIST_CARD_CLASS}
        onClick={onSelect}
      >
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-surface-inset/40">
          <Icon className="h-4 w-4 text-muted-foreground" aria-hidden />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <p className="truncate text-sm font-medium text-foreground">
              {providerName}
            </p>
            <span
              className={cn(
                "inline-flex h-2 w-2 shrink-0 rounded-full",
                keyConfigured ? "bg-success" : "bg-amber-500",
              )}
              aria-label={keyConfigured ? "Key 已配置" : "需要配置 Key"}
            />
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {providerModels.length} 个已启用模型
          </p>
          {providerModels.length > 0 ? (
            <div className="mt-2 flex flex-wrap items-center gap-1.5">
              {visibleModels.map((model) => (
                <span
                  key={model.id}
                  className="inline-flex max-w-full items-center gap-1 rounded-md border border-border/55 bg-background/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground"
                >
                  <span className="truncate">{model.id}</span>
                  {llmModelShowsVisionBadge(model) ? (
                    <Eye
                      className="h-3 w-3 shrink-0 text-muted-foreground"
                      aria-label="已通过视觉验证"
                    />
                  ) : null}
                </span>
              ))}
              {hiddenModelCount > 0 ? (
                <span className="text-[11px] text-muted-foreground">
                  +{hiddenModelCount}
                </span>
              ) : null}
            </div>
          ) : null}
        </div>
        <ChevronRight
          className="h-4 w-4 shrink-0 text-muted-foreground"
          aria-hidden
        />
      </button>
    </Button>
  );
}
