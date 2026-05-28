import { SceneSelector } from "@/components/ai/SceneSelector";
import type { AiScene } from "@/types/ai";

interface AiPanelHeaderProps {
  scene: AiScene;
  onSceneChange: (scene: AiScene) => void;
}

export function AiPanelHeader({ scene, onSceneChange }: AiPanelHeaderProps) {
  return (
    <div className="flex shrink-0 items-center border-b border-border/60 px-3 py-2.5">
      <SceneSelector scene={scene} onSceneChange={onSceneChange} />
    </div>
  );
}
