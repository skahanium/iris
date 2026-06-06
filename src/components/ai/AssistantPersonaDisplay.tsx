import { AssistantAvatar } from "@/components/ai/AssistantAvatar";
import { profileToAvatarIdentity } from "@/lib/prompt-profile";
import type { PromptProfileDto } from "@/lib/ipc";

interface AssistantPersonaDisplayProps {
  profile: PromptProfileDto;
}

/** 侧栏只读：头像 + 称呼 */
export function AssistantPersonaDisplay({
  profile,
}: AssistantPersonaDisplayProps) {
  const identity = profileToAvatarIdentity(profile);

  return (
    <div
      className="flex min-w-0 items-center gap-2"
      data-testid="assistant-persona-display"
    >
      <AssistantAvatar identity={identity} />
      <span className="truncate text-sm font-medium text-foreground">
        {identity.displayName}
      </span>
    </div>
  );
}
