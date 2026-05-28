import { useCallback, useState } from "react";
import { Loader2, ShieldCheck } from "lucide-react";

import { CitationCheckView } from "@/components/ai/CitationCheckView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invokeErrorMessage } from "@/lib/credentials";
import { citationCheck } from "@/lib/ipc";
import type { CitationCheckResult } from "@/types/ai";

interface CitationTaskPanelProps {
  notePath: string | null;
  getParagraphText: () => string | null;
  webSearch?: boolean;
}

export function CitationTaskPanel({
  notePath,
  getParagraphText,
  webSearch = false,
}: CitationTaskPanelProps) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CitationCheckResult | null>(null);

  const runCheck = useCallback(async () => {
    if (!notePath) return;
    const text = getParagraphText();
    if (!text?.trim()) {
      setError("请先在编辑器中选中要检查的段落");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await citationCheck({
        paragraph_text: text,
        document_path: notePath,
        web_authorized: webSearch,
      });
      setResult(res as CitationCheckResult);
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [notePath, getParagraphText, webSearch]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden p-3">
      <Card className="shrink-0 border-border/60">
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">检查引用</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          <Button
            type="button"
            size="sm"
            className="w-full"
            disabled={loading || !notePath}
            onClick={() => void runCheck()}
          >
            {loading ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : (
              <ShieldCheck className="mr-1 h-3.5 w-3.5" />
            )}
            检查当前选区
          </Button>
          {error ? (
            <p className="text-xs text-destructive">{error}</p>
          ) : null}
        </CardContent>
      </Card>
      {result ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <CitationCheckView result={result} />
        </div>
      ) : null}
    </div>
  );
}
