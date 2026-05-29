import { Sparkles } from "lucide-react";

import {
  assistantInitial,
  type AssistantIdentity,
} from "@/lib/assistant-identity";
import { cn } from "@/lib/utils";

interface AssistantAvatarProps {
  identity: AssistantIdentity;
  className?: string;
}

export function AssistantAvatar({ identity, className }: AssistantAvatarProps) {
  const box = cn(
    "flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-primary/10",
    className,
  );

  if (identity.avatarEmoji) {
    return (
      <span className={box} aria-hidden>
        <span className="text-base leading-none">{identity.avatarEmoji}</span>
      </span>
    );
  }

  const initial = assistantInitial(identity.displayName);
  if (initial) {
    return (
      <span
        className={cn(box, "text-sm font-semibold text-primary")}
        aria-hidden
      >
        {initial}
      </span>
    );
  }

  return (
    <span className={cn(box, "text-primary")} aria-hidden>
      <Sparkles className="h-4 w-4" />
    </span>
  );
}
