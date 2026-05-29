import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface DocumentCheckArtifactsProps {
  summary: string | null;
  issues: string[];
}

export function DocumentCheckArtifacts({
  summary,
  issues,
}: DocumentCheckArtifactsProps) {
  if (!summary && issues.length === 0) return null;

  return (
    <div className="space-y-2">
      {summary ? (
        <Card className="border-border/60">
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">文档检查摘要</CardTitle>
          </CardHeader>
          <CardContent className="whitespace-pre-wrap text-xs text-muted-foreground">
            {summary}
          </CardContent>
        </Card>
      ) : null}
      {issues.length > 0 ? (
        <Card className="border-border/60">
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">发现问题</CardTitle>
          </CardHeader>
          <CardContent>
            <ul className="max-h-40 list-disc space-y-1 overflow-y-auto pl-4 text-xs text-muted-foreground">
              {issues.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
