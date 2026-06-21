import { ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

interface AssistantArtifactTagStripProps {
  artifacts: AssistantArtifactDraft[];
  onOpenArtifact: (draft: AssistantArtifactDraft) => void;
}

export function AssistantArtifactTagStrip({
  artifacts,
  onOpenArtifact,
}: AssistantArtifactTagStripProps) {
  if (!artifacts.length) return null;

  return (
    <div
      className="flex flex-wrap gap-1.5 px-3 py-2"
      data-testid="assistant-artifact-tags"
    >
      {artifacts.map((artifact) => (
        <Button
          key={`${artifact.kind}:${artifact.sourceRequestId}:${artifact.title}`}
          type="button"
          size="sm"
          variant="outline"
          className="h-7 gap-1 rounded-full px-2.5 text-xs"
          onClick={() => onOpenArtifact(artifact)}
        >
          {artifact.title}
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
      ))}
    </div>
  );
}
