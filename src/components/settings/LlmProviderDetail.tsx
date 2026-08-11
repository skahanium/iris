import { ChevronDown } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { isCustomProviderId, type ProviderOverride } from "@/types/llm";

import type {
  LlmEnabledProviderModel,
  LlmVisibleProvider,
} from "./llmProviderTypes";

function ModelDebugDetails({
  model,
  chatOnly,
}: {
  model: LlmEnabledProviderModel["catalog"];
  chatOnly: boolean;
}) {
  if (!model) {
    return (
      <details className="text-[10px] text-muted-foreground">
        <summary className="cursor-pointer select-none">详情</summary>
        <div className="mt-1 flex flex-wrap items-center gap-1">
          <span className="rounded border border-border/50 px-1.5 py-0.5">
            manual model
          </span>
          {chatOnly ? (
            <span className="rounded border border-border/50 px-1.5 py-0.5">
              chat-only
            </span>
          ) : null}
        </div>
      </details>
    );
  }
  const tags = [
    model.supportsVision ? "vision" : null,
    model.supportsTools ? "tools" : null,
    model.supportsStreaming ? "streaming" : null,
    model.supportsThinking ? "reasoning" : null,
    chatOnly ? "chat-only" : null,
    `${Math.round(model.contextWindow / 1000)}k ctx`,
    model.endpointFamily,
  ].filter((tag): tag is string => Boolean(tag));

  return (
    <details className="text-[10px] text-muted-foreground">
      <summary className="cursor-pointer select-none">详情</summary>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        {tags.map((tag) => (
          <span
            key={tag}
            className="rounded border border-border/50 px-1.5 py-0.5"
          >
            {tag}
          </span>
        ))}
      </div>
    </details>
  );
}

export interface LlmProviderDetailProps {
  provider: LlmVisibleProvider;
  override: ProviderOverride | undefined;
  providerModels: LlmEnabledProviderModel[];
  providerResult: { ok: boolean; message: string } | undefined;
  requiresBaseUrl: boolean;
  baseUrl: string;
  keyInput: string;
  keyConfigured: boolean;
  keySaving: boolean;
  testing: string | null;
  refreshingProvider: string | null;
  newModelInput: string;
  testResults: Record<string, { ok: boolean; message: string }>;
  modelSummary: (
    model: LlmEnabledProviderModel,
    result: { ok: boolean; message: string } | undefined,
    reasoningSummary: string,
  ) => string;
  reasoningSummaryForModel: (modelId: string) => string;
  onKeyInput: (value: string) => void;
  onSaveKey: () => void;
  onClearKey: () => void;
  onTestProvider: () => void;
  onRefreshModels: () => void;
  onDeleteProvider: () => void;
  onBaseUrlChange: (url: string) => void;
  onLabelChange: (label: string) => void;
  onNewModelInputChange: (value: string) => void;
  onAddModel: () => void;
  onValidateModel: (model: LlmEnabledProviderModel) => void;
  onRemoveModel: (modelId: string) => void;
}

export function LlmProviderDetail({
  provider,
  override,
  providerModels,
  providerResult,
  requiresBaseUrl,
  baseUrl,
  keyInput,
  keyConfigured,
  keySaving,
  testing,
  refreshingProvider,
  newModelInput,
  testResults,
  modelSummary,
  reasoningSummaryForModel,
  onKeyInput,
  onSaveKey,
  onClearKey,
  onTestProvider,
  onRefreshModels,
  onDeleteProvider,
  onBaseUrlChange,
  onLabelChange,
  onNewModelInputChange,
  onAddModel,
  onValidateModel,
  onRemoveModel,
}: LlmProviderDetailProps) {
  const [advancedOpen, setAdvancedOpen] = useState(false);

  return (
    <div
      className="space-y-4"
      data-testid="llm-provider-detail"
      data-provider-id={provider.id}
    >
      <section className="space-y-3 rounded-md border border-border/55 bg-background/60 p-3">
        <p className="text-xs font-medium text-foreground">连接与凭据</p>
        {provider.requiresApiKey ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <Input
                type="password"
                className="h-8 w-44 text-xs"
                placeholder="API Key…"
                value={keyInput}
                onChange={(event) => onKeyInput(event.target.value)}
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                className="h-8"
                disabled={keySaving}
                onClick={onSaveKey}
              >
                保存 Key
              </Button>
              {keyConfigured ? (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-8"
                  onClick={onClearKey}
                >
                  清除
                </Button>
              ) : null}
            </div>
          </>
        ) : (
          <p className="text-[11px] text-muted-foreground">
            本机服务无需 API Key
          </p>
        )}
        <p className="text-[11px] text-muted-foreground">
          {keyConfigured ? "Key 已配置" : "需要配置 Key"}
          {" · "}
          检查、刷新、验证会优先使用当前输入框 Key；留空则使用已保存 Key。
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="h-7 text-xs"
            disabled={testing === provider.id}
            onClick={onTestProvider}
          >
            {testing === provider.id ? "检查中…" : "检查端点"}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7 text-xs"
            disabled={refreshingProvider === provider.id}
            onClick={onRefreshModels}
          >
            {refreshingProvider === provider.id ? "刷新中…" : "刷新模型"}
          </Button>
        </div>
        {providerResult ? (
          <p
            className={
              providerResult.ok
                ? "text-[11px] text-success"
                : "text-[11px] text-destructive"
            }
          >
            {providerResult.message}
          </p>
        ) : null}
      </section>

      <section className="space-y-2" data-testid="llm-provider-enabled-models">
        <p className="text-xs font-medium text-foreground">已启用模型</p>
        <div className="flex flex-wrap items-center gap-2">
          <Input
            className="h-8 min-w-48 flex-1 text-xs"
            placeholder="模型 ID，如 deepseek-v4-flash"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            value={newModelInput}
            onChange={(event) => onNewModelInputChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                onAddModel();
              }
            }}
          />
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="h-8 text-xs"
            onClick={onAddModel}
          >
            添加模型
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">
          可一次粘贴多个模型 ID，用逗号或换行分隔；同一个 Key 会被这些模型共享。
        </p>
        {providerModels.length === 0 ? (
          <p className="rounded-md border border-dashed border-border/50 px-3 py-2 text-[11px] text-muted-foreground">
            未添加模型时不会激活或展示任何模型。
          </p>
        ) : (
          providerModels.map((model) => {
            const result = testResults[`${provider.id}:${model.id}`];
            const reasoningSummary = reasoningSummaryForModel(model.id);
            const summary = modelSummary(model, result, reasoningSummary);
            const modelTesting = testing === `${provider.id}:${model.id}`;
            return (
              <div
                key={model.id}
                className="rounded-md border border-border/45 bg-background/50 p-2"
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <span className="block truncate font-mono text-xs font-medium text-foreground">
                      {model.id}
                    </span>
                    {model.catalog?.displayName ? (
                      <span className="block truncate text-[11px] text-muted-foreground">
                        {model.catalog.displayName}
                      </span>
                    ) : null}
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      className="h-7 text-xs"
                      disabled={modelTesting}
                      onClick={() => onValidateModel(model)}
                    >
                      {modelTesting ? "验证中…" : "验证模型"}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="h-7 text-xs"
                      onClick={() => onRemoveModel(model.id)}
                    >
                      移除
                    </Button>
                  </div>
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <span
                    className={
                      result?.ok === false
                        ? "text-[11px] text-destructive"
                        : "text-[11px] text-muted-foreground"
                    }
                  >
                    {isCustomProviderId(provider.id) && !result
                      ? `${summary} · Chat-only（Agent 协议未验证）`
                      : summary}
                  </span>
                  <ModelDebugDetails
                    model={model.catalog}
                    chatOnly={isCustomProviderId(provider.id)}
                  />
                </div>
              </div>
            );
          })
        )}
      </section>

      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <CollapsibleTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5 text-xs text-muted-foreground"
            data-testid="llm-provider-advanced-trigger"
          >
            <ChevronDown
              className={cn(
                "h-4 w-4 transition-transform duration-fast",
                advancedOpen && "rotate-180",
              )}
            />
            高级设置
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-2 space-y-3 rounded-md border border-border-subtle bg-surface-inset/20 p-3">
          {isCustomProviderId(provider.id) ? (
            <label className="block space-y-1 text-xs font-medium text-foreground">
              显示名称
              <Input
                className="h-8 text-xs"
                placeholder="显示名称"
                defaultValue={override?.label ?? provider.name}
                onBlur={(event) => onLabelChange(event.target.value.trim())}
              />
            </label>
          ) : null}
          {requiresBaseUrl ? (
            <label className="block space-y-1 text-xs font-medium text-foreground">
              自定义端点 Base URL
              <Input
                className="h-8 text-xs"
                placeholder="自定义端点 Base URL"
                value={baseUrl}
                onChange={(event) => onBaseUrlChange(event.target.value)}
              />
            </label>
          ) : (
            <p className="text-[11px] text-muted-foreground">
              内置供应商使用系统默认端点
            </p>
          )}
          {provider.configured ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 text-xs text-destructive"
              onClick={onDeleteProvider}
            >
              删除供应商
            </Button>
          ) : null}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
