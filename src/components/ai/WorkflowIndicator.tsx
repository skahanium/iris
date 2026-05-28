import type { AiScene, ContextStatus } from "@/types/ai";
import { SCENE_META } from "@/lib/ai/scene-types";

interface WorkflowIndicatorProps {
  scene: AiScene;
  contextStatus: ContextStatus | null;
  noteDisplayTitle: string | null;
}

export function WorkflowIndicator({
  scene,
  contextStatus,
  noteDisplayTitle,
}: WorkflowIndicatorProps) {
  const meta = SCENE_META[scene];
  const isGlobal = meta.defaultScope === "global";

  const parts: string[] = [meta.label];

  if (isGlobal) {
    parts.push("库级");
  } else if (noteDisplayTitle) {
    parts.push(noteDisplayTitle);
  }

  if (contextStatus) {
    const loaded: string[] = [];
    if (contextStatus.regulations_loaded > 0)
      loaded.push(`${contextStatus.regulations_loaded} 部法规`);
    if (contextStatus.anchors_loaded > 0)
      loaded.push(`${contextStatus.anchors_loaded} 条锚点`);
    if (contextStatus.links_loaded > 0)
      loaded.push(`${contextStatus.links_loaded} 条链接`);
    if (loaded.length > 0) {
      parts.push(`已加载: ${loaded.join(" · ")}`);
    }
  }

  return (
    <div className="flex items-center gap-2 border-b border-border/60 bg-surface-inset/40 px-3 py-1.5 text-xs text-muted-foreground">
      <span
        className="inline-block h-1.5 w-1.5 rounded-full bg-primary/80"
        title="Agent 就绪"
      />
      <span>{parts.join(" · ")}</span>
    </div>
  );
}
