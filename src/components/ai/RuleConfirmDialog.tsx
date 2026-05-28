import { useState, useCallback } from "react";
import { Check, X, AlertTriangle, Ruler } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

// ─── Types ───────────────────────────────────────────────

export interface RuleConfirmRequest {
  rule: string;
  rule_type: string;
  source: string;
}

const RULE_TYPE_LABELS: Record<string, string> = {
  output_format: "输出格式",
  citation_style: "引用风格",
  tone: "语气",
  tool_preference: "工具偏好",
  agent_behavior: "AI 行为",
};

// ─── Component ───────────────────────────────────────────

interface RuleConfirmDialogProps {
  request: RuleConfirmRequest | null;
  onConfirm: (request: RuleConfirmRequest) => void;
  onReject: () => void;
  onClose: () => void;
}

export function RuleConfirmDialog({
  request,
  onConfirm,
  onReject,
  onClose,
}: RuleConfirmDialogProps) {
  const [confirmed, setConfirmed] = useState(false);

  const handleConfirm = useCallback(() => {
    if (!request) return;
    setConfirmed(true);
    onConfirm(request);
    setTimeout(() => {
      setConfirmed(false);
      onClose();
    }, 1500);
  }, [request, onConfirm, onClose]);

  const handleReject = useCallback(() => {
    onReject();
    onClose();
  }, [onReject, onClose]);

  if (!request) return null;

  const typeLabel = RULE_TYPE_LABELS[request.rule_type] ?? request.rule_type;

  return (
    <Dialog open={!!request} onOpenChange={() => onClose()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Ruler className="h-5 w-5 text-primary" />
            保存规则
          </DialogTitle>
          <DialogDescription>
            AI 建议保存以下规则到您的个人偏好中。此规则将在后续对话中生效。
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <Badge variant="secondary">{typeLabel}</Badge>
            <span className="text-xs text-muted-foreground">
              来源: {request.source}
            </span>
          </div>

          <div className="rounded-md border border-primary/20 bg-primary/5 p-3">
            <p className="text-sm">{request.rule}</p>
          </div>

          <div className="flex items-start gap-2 rounded-md border border-muted bg-muted/30 p-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
            <p className="text-xs text-muted-foreground">
              保存后可在"设置 → AI 记忆与规则"中查看、停用或删除此规则。
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={handleReject}>
            <X className="mr-1 h-4 w-4" />
            跳过
          </Button>
          <Button size="sm" onClick={handleConfirm} disabled={confirmed}>
            {confirmed ? (
              <>
                <Check className="mr-1 h-4 w-4" />
                已保存
              </>
            ) : (
              <>
                <Check className="mr-1 h-4 w-4" />
                确认保存
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
