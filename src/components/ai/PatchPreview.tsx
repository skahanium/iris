import { useCallback, useState } from "react";
import { AlertTriangle, Check, Copy, RefreshCw, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { MarkdownRenderable } from "@/components/ai/MarkdownRenderable";
import { cn } from "@/lib/utils";
import type { PatchProposal } from "@/types/ai";

// ─── Risk Level Styling ─────────────────────────────────

const RISK_STYLES: Record<string, { label: string; className: string }> = {
  low: { label: "低风险", className: "bg-green-500/10 text-green-600" },
  medium: { label: "中风险", className: "bg-yellow-500/10 text-yellow-600" },
  high: { label: "高风险", className: "bg-red-500/10 text-red-600" },
};

// ─── Component ──────────────────────────────────────────

interface PatchPreviewProps {
  patch: PatchProposal;
  onAccept: (patch: PatchProposal) => void;
  onReject: (patch: PatchProposal) => void;
  onCopy: (patch: PatchProposal) => void;
  onRegenerate: (patch: PatchProposal) => void;
}

export interface DiffViewProps {
  beforeText: string;
  afterText: string;
  patchType: "insert" | "replace";
  riskLevel: "low" | "medium" | "high";
  targetPath: string;
}

export function DiffView({
  beforeText,
  afterText,
  patchType,
  riskLevel,
  targetPath,
}: DiffViewProps) {
  const [showFullDiff, setShowFullDiff] = useState(false);
  const riskStyle = RISK_STYLES[riskLevel] ?? RISK_STYLES.low!;
  const beforeLines = beforeText.split("\n");
  const afterLines = afterText.split("\n");
  const maxLines = Math.max(beforeLines.length, afterLines.length);
  const displayLines = showFullDiff ? maxLines : Math.min(5, maxLines);
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {targetPath} {patchType === "insert" ? "(插入)" : "(替换)"}
        </span>
        <Badge variant="outline" className={cn("text-xs", riskStyle.className)}>
          {riskStyle.label}
        </Badge>
      </div>
      <div className="overflow-hidden rounded-md border border-border/60">
        <div className="max-h-[200px] overflow-auto font-mono text-xs">
          <div className="border-b border-border/40">
            <div className="bg-red-500/5 px-3 py-1 text-red-600">- 原文</div>
            {beforeLines.slice(0, displayLines).map((line, i) => (
              <div key={i} className="px-3 py-0.5 text-red-600/80">
                <span className="mr-2 select-none text-red-400">-</span>
                {line || " "}
              </div>
            ))}
            {!showFullDiff && beforeLines.length > displayLines && (
              <div className="px-3 py-0.5 text-muted-foreground">
                ... 还有 {beforeLines.length - displayLines} 行
              </div>
            )}
          </div>
          <div>
            <div className="bg-green-500/5 px-3 py-1 text-green-600">
              + 改后
            </div>
            {afterLines.slice(0, displayLines).map((line, i) => (
              <div key={i} className="px-3 py-0.5 text-green-600/80">
                <span className="mr-2 select-none text-green-400">+</span>
                {line || " "}
              </div>
            ))}
            {!showFullDiff && afterLines.length > displayLines && (
              <div className="px-3 py-0.5 text-muted-foreground">
                ... 还有 {afterLines.length - displayLines} 行
              </div>
            )}
          </div>
        </div>
        {maxLines > 5 && (
          <button
            type="button"
            className="w-full border-t border-border/40 bg-muted/30 px-3 py-1 text-xs"
            onClick={() => setShowFullDiff(!showFullDiff)}
          >
            {showFullDiff ? "收起" : "展开全部 " + maxLines + " 行"}
          </button>
        )}
      </div>
    </div>
  );
}

export function PatchPreview({
  patch,
  onAccept,
  onReject,
  onCopy,
  onRegenerate,
}: PatchPreviewProps) {
  const [showFullDiff, setShowFullDiff] = useState(false);

  const riskStyle = RISK_STYLES[patch.risk_level] ?? RISK_STYLES.low!;

  const handleAccept = useCallback(() => {
    onAccept(patch);
  }, [patch, onAccept]);

  const handleReject = useCallback(() => {
    onReject(patch);
  }, [patch, onReject]);

  const handleCopy = useCallback(() => {
    onCopy(patch);
  }, [patch, onCopy]);

  const handleRegenerate = useCallback(() => {
    onRegenerate(patch);
  }, [patch, onRegenerate]);

  // Compute diff preview
  const originalLines = patch.original_text.split("\n");
  const replacementLines = patch.replacement_text.split("\n");
  const maxLines = Math.max(originalLines.length, replacementLines.length);
  const displayLines = showFullDiff ? maxLines : Math.min(5, maxLines);

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-medium">补丁建议</CardTitle>
          <div className="flex items-center gap-2">
            <Badge
              variant="outline"
              className={cn("text-xs", riskStyle.className)}
            >
              {riskStyle.label}
            </Badge>
            <Badge variant="outline" className="text-xs">
              {patch.evidence_packet_ids.length} 条证据
            </Badge>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* Warnings */}
        {patch.warnings.length > 0 && (
          <div className="space-y-1">
            {patch.warnings.map((warning, i) => (
              <div
                key={i}
                className="flex items-start gap-2 rounded-md bg-yellow-500/5 px-2 py-1.5 text-xs text-yellow-600"
              >
                <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                <MarkdownRenderable
                  content={warning}
                  profile="patch_preview"
                  className="text-xs"
                />
              </div>
            ))}
          </div>
        )}

        {/* Diff Preview */}
        <div className="overflow-hidden rounded-md border border-border/60">
          <div className="flex items-center justify-between bg-muted/50 px-3 py-1.5 text-xs text-muted-foreground">
            <span>差异预览</span>
            <span>
              字符 {patch.range.start}-{patch.range.end}
            </span>
          </div>
          <div className="max-h-[200px] overflow-auto font-mono text-xs">
            {/* Original text */}
            <div className="border-b border-border/40">
              <div className="bg-red-500/5 px-3 py-1 text-red-600">- 原文</div>
              {originalLines.slice(0, displayLines).map((line, i) => (
                <div key={i} className="px-3 py-0.5 text-red-600/80">
                  <span className="mr-2 select-none text-red-400">-</span>
                  {line || " "}
                </div>
              ))}
              {!showFullDiff && originalLines.length > displayLines && (
                <div className="px-3 py-0.5 text-muted-foreground">
                  ... 还有 {originalLines.length - displayLines} 行
                </div>
              )}
            </div>

            {/* Replacement text */}
            <div>
              <div className="bg-green-500/5 px-3 py-1 text-green-600">
                + 替换
              </div>
              {replacementLines.slice(0, displayLines).map((line, i) => (
                <div key={i} className="px-3 py-0.5 text-green-600/80">
                  <span className="mr-2 select-none text-green-400">+</span>
                  {line || " "}
                </div>
              ))}
              {!showFullDiff && replacementLines.length > displayLines && (
                <div className="px-3 py-0.5 text-muted-foreground">
                  ... 还有 {replacementLines.length - displayLines} 行
                </div>
              )}
            </div>
          </div>

          {/* Show more/less toggle */}
          {maxLines > 5 && (
            <button
              type="button"
              className="w-full border-t border-border/40 bg-muted/30 px-3 py-1 text-xs text-muted-foreground hover:bg-muted/50"
              onClick={() => setShowFullDiff(!showFullDiff)}
            >
              {showFullDiff ? "收起" : `展开全部 ${maxLines} 行`}
            </button>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-between pt-1">
          <div className="flex gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleRegenerate}
              title="重新生成"
            >
              <RefreshCw className="mr-1 h-3.5 w-3.5" />
              重新生成
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleCopy}
              title="复制替换文本"
            >
              <Copy className="mr-1 h-3.5 w-3.5" />
              复制
            </Button>
          </div>
          <div className="flex gap-1.5">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={handleReject}
              title="拒绝补丁"
            >
              <X className="mr-1 h-3.5 w-3.5" />
              拒绝
            </Button>
            <Button
              type="button"
              size="sm"
              onClick={handleAccept}
              title="接受补丁"
            >
              <Check className="mr-1 h-3.5 w-3.5" />
              接受
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
