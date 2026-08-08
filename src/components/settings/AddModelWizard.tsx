import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { isCustomProviderId, type LlmConfigGetResponse } from "@/types/llm";

export function AddModelWizard({
  providers,
  keyConfigured,
  keyInputsRef,
  keySaving,
  onKeyInput,
  onSaveKey,
  onCreateCustom,
  onBaseUrl,
  onLabel,
  onClose,
}: {
  providers: LlmConfigGetResponse["providers"];
  keyConfigured: Record<string, boolean>;
  keyInputsRef: React.RefObject<Record<string, string>>;
  keySaving: string | null;
  onKeyInput: (id: string, value: string) => void;
  onSaveKey: (id: string) => void;
  onCreateCustom: () => string | null;
  onBaseUrl: (id: string, url: string) => void;
  onLabel: (id: string, label: string) => void;
  onClose: () => void;
}) {
  const [providerId, setProviderId] = useState(providers[0]?.id ?? "deepseek");
  const selectedProvider = providers.find(
    (provider) => provider.id === providerId,
  );
  const custom =
    isCustomProviderId(providerId) ||
    selectedProvider?.endpointManaged === "custom";

  const createCustom = () => {
    const id = onCreateCustom();
    if (id) setProviderId(id);
  };

  return (
    <div className="rounded-md border border-border/60 bg-surface-inset/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <div>
          <p className="text-xs font-semibold text-foreground">添加供应商</p>
          <p className="mt-1 text-[11px] text-muted-foreground">
            未配置厂商只在这里选择；保存后才进入主列表。
          </p>
        </div>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          收起
        </Button>
      </div>
      <div className="mt-3 grid gap-2 lg:grid-cols-[1fr_auto]">
        <Select value={providerId} onValueChange={setProviderId}>
          <SelectTrigger className="h-8 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {providers.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-8"
          onClick={createCustom}
        >
          自定义端点
        </Button>
      </div>
      {custom ? (
        <div className="mt-2 grid gap-2 lg:grid-cols-2">
          <Input
            className="h-8 text-xs"
            placeholder="显示名称"
            onBlur={(event) => onLabel(providerId, event.target.value)}
          />
          <Input
            className="h-8 text-xs"
            placeholder="自定义端点 Base URL"
            onBlur={(event) => onBaseUrl(providerId, event.target.value)}
          />
        </div>
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <Input
          type="password"
          className="h-8 max-w-sm text-xs"
          placeholder="API Key…"
          value={keyInputsRef.current?.[providerId] ?? ""}
          onChange={(event) => onKeyInput(providerId, event.target.value)}
        />
        <Button
          type="button"
          size="sm"
          className="h-8"
          disabled={keySaving === providerId}
          onClick={() => onSaveKey(providerId)}
        >
          {keySaving === providerId ? "保存中…" : "保存 Key"}
        </Button>
        <span className="text-[11px] text-muted-foreground">
          {keyConfigured[providerId] ? "Key 已配置" : "保存后显示在主列表"}
        </span>
      </div>
    </div>
  );
}
