import type { AvatarIdentity } from "@/lib/prompt-profile";
import { cn } from "@/lib/utils";

interface AssistantAvatarProps {
  identity: AvatarIdentity;
  className?: string;
}

function AvatarGlyph({ avatarId }: Pick<AvatarIdentity, "avatarId">) {
  switch (avatarId) {
    case "orbit":
      return <ellipse cx="16" cy="16" rx="9" ry="5.5" />;
    case "axis":
      return <path d="M9 23 16 9l7 14M12.5 18h7" />;
    case "frame":
      return <path d="M10 10h12v12H10zM13 13h6v6h-6z" />;
    case "lens":
      return <path d="m11 19 5-8 5 8-5 3-5-3Z" />;
    case "grid":
      return <path d="M10 10h12v12H10zM16 10v12M10 16h12" />;
    case "flow":
      return <path d="M10 12c3-3 6-3 12 0M10 20c3 3 6 3 12 0" />;
    case "signal":
      return <path d="M10 20v-3m4 3v-6m4 6v-9m4 9v-12" />;
    case "iris":
      return <path d="M10 10h12M16 10v12M12 22h8" />;
  }
}

/** Compact, monochrome Iris geometry used for every assistant identity. */
export function AssistantAvatar({ identity, className }: AssistantAvatarProps) {
  return (
    <span
      className={cn(
        "flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border-subtle bg-surface-inset/40 text-foreground/75",
        className,
      )}
      data-avatar-id={identity.avatarId}
      data-testid="assistant-avatar"
      aria-hidden
    >
      <svg
        viewBox="0 0 32 32"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.65"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="h-6 w-6"
      >
        <rect x="2.5" y="2.5" width="27" height="27" rx="7.5" opacity="0.42" />
        <AvatarGlyph avatarId={identity.avatarId} />
      </svg>
    </span>
  );
}
