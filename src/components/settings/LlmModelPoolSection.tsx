//! Primary-to-fallback model pool ordering editor.
//!
//! Extracted from LlmRoutingSection so the routing section stays readable.

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

import { modelReferenceValue } from "./llmRoutingModelHelpers";

export interface LlmPoolModelReference {
  providerId: string;
  modelId: string;
  label: string;
}

export function LlmModelPoolSection({
  orderedModelReferences,
  saving,
  loadError,
  message,
  onMove,
  onSave,
}: {
  orderedModelReferences: LlmPoolModelReference[];
  saving: boolean;
  loadError: string | null;
  message: string | null;
  onMove: (index: number, delta: -1 | 1) => void;
  onSave: () => void;
}) {
  return (
    <>
      <section className="space-y-2" data-section="llm-model-pool">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            模型池与主备顺序
          </p>
        </div>
        <p className="text-[11px] text-muted-foreground">
          第一项是主模型，后两项是备用模型。任务先按文本、工具、视觉、推理和上下文能力过滤，再按此顺序最多尝试三个候选；显式指定模型的请求不会自动切换。
        </p>
        {orderedModelReferences.length === 0 ? (
          <Input
            className="h-8 text-xs"
            value=""
            placeholder="先在供应商配置中添加并启用模型"
            disabled
          />
        ) : (
          <div className="space-y-1">
            {orderedModelReferences.map((model, index) => (
              <div
                key={modelReferenceValue(model.providerId, model.modelId)}
                className="flex items-center gap-2 rounded-md border border-border/50 px-2 py-1.5 text-xs"
              >
                <span className="w-16 shrink-0 text-muted-foreground">
                  {index === 0 ? "主模型" : `备用 ${index}`}
                </span>
                <span className="min-w-0 flex-1 truncate">{model.label}</span>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2 text-xs"
                  disabled={index === 0}
                  onClick={() => onMove(index, -1)}
                >
                  上移
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="h-6 px-2 text-xs"
                  disabled={index === orderedModelReferences.length - 1}
                  onClick={() => onMove(index, 1)}
                >
                  下移
                </Button>
              </div>
            ))}
          </div>
        )}
      </section>

      <div className="flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={saving || Boolean(loadError)}
          onClick={onSave}
        >
          {saving ? "保存中…" : "保存模型池"}
        </Button>
        {message ? (
          <span className="text-xs text-muted-foreground">{message}</span>
        ) : null}
      </div>
    </>
  );
}
