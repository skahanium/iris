import { FilePenLine } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { WritingState } from "@/types/ai";

interface WritingStatePanelProps {
  state: WritingState | null;
}

export function WritingStatePanel({ state }: WritingStatePanelProps) {
  if (!state) return null;
  const firstRevision = state.revision_records[0];

  return (
    <div className="ai-task-surface px-3 pt-3" data-testid="writing-state">
      <Card className="border-border/60">
        <CardHeader className="p-3 pb-2">
          <CardTitle className="flex items-center gap-2 text-sm">
            <FilePenLine className="h-4 w-4" />
            文稿状态
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 p-3 pt-0 text-xs">
          <p className="font-medium">{state.document_goal}</p>
          <div className="flex flex-wrap gap-1.5">
            {state.audience ? (
              <Badge variant="outline">{state.audience}</Badge>
            ) : null}
            {state.genre ? (
              <Badge variant="outline">{state.genre}</Badge>
            ) : null}
            {state.style_constraints.slice(0, 3).map((style) => (
              <Badge key={style} variant="secondary">
                {style}
              </Badge>
            ))}
          </div>
          <p className="text-muted-foreground">
            草稿版本 {state.draft_version_hash.slice(0, 12)}
          </p>
          {firstRevision ? (
            <div className="rounded-md border border-border/60 px-2 py-1.5">
              <p>{firstRevision.reason}</p>
              <p className="mt-1 text-muted-foreground">
                风险 {firstRevision.risk} · {firstRevision.rollback}
              </p>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
