import { FileText, Layers, ListChecks, MessageSquare, PenSquare, Quote } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { UnifiedArtifact } from "@/lib/map-harness-result-to-artifacts";
import { cn } from "@/lib/utils";

interface AssistantArtifactPanelProps {
  artifacts: UnifiedArtifact[];
  className?: string;
}

function artifactIcon(kind: UnifiedArtifact["kind"]) {
  switch (kind) {
    case "message":
      return MessageSquare;
    case "patches":
      return PenSquare;
    case "citation_report":
      return Quote;
    case "document_check":
      return ListChecks;
    case "chapter_writing":
      return Layers;
    default:
      return FileText;
  }
}

export function AssistantArtifactPanel({
  artifacts,
  className,
}: AssistantArtifactPanelProps) {
  if (artifacts.length === 0) return null;

  return (
    <div className={cn("space-y-2 px-3 pt-3", className)} data-testid="artifact-panel">
      <Card className="border-border/60">
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">任务产出</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          {artifacts.map((item) => {
            const Icon = artifactIcon(item.kind);
            return (
              <div
                key={item.id}
                className="flex items-start gap-2 rounded-md border border-border/60 px-3 py-2"
              >
                <Icon className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-xs font-medium">{item.title}</span>
                    <Badge variant="outline" className="text-[10px]">
                      {item.kind}
                    </Badge>
                    <Badge
                      variant={
                        item.status === "pending" ? "secondary" : "outline"
                      }
                      className="text-[10px]"
                    >
                      {item.status}
                    </Badge>
                    {item.evidenceCount > 0 ? (
                      <span className="text-[10px] text-muted-foreground">
                        证据 {item.evidenceCount}
                      </span>
                    ) : null}
                  </div>
                </div>
              </div>
            );
          })}
        </CardContent>
      </Card>
    </div>
  );
}
