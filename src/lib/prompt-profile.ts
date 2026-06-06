/** PromptProfile 前端工具与迁移（单一数据源：SQLite via IPC） */

import type { PromptProfileDto } from "@/lib/ipc";

export const DEFAULT_DISPLAY_NAME = "砚";

export const DEFAULT_PROMPT_PROFILE: PromptProfileDto = {
  display_name: DEFAULT_DISPLAY_NAME,
  avatar_emoji: null,
  persona: "",
  writing_style: "",
  custom_rules: [],
  language: "zh-CN",
};

const LEGACY_IDENTITY_STORAGE_KEY = "iris-assistant-identity";
const MAX_NAME_LENGTH = 24;

export const PROMPT_PROFILE_CHANGED = "iris-prompt-profile-changed";

export interface AvatarIdentity {
  displayName: string;
  avatarEmoji: string | null;
}

export function sanitizeDisplayName(value: string): string {
  return value.trim().slice(0, MAX_NAME_LENGTH);
}

/** 仅保留首个 grapheme（emoji 安全） */
export function sanitizeAvatarEmoji(
  value: string | null | undefined,
): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed) return null;
  const [first] = [...trimmed];
  return first ?? null;
}

export function assistantInitial(displayName: string): string {
  const trimmed = displayName.trim();
  if (!trimmed) return "砚";
  return [...trimmed][0] ?? "砚";
}

export function profileToAvatarIdentity(
  profile: PromptProfileDto,
): AvatarIdentity {
  const displayName =
    sanitizeDisplayName(profile.display_name) || DEFAULT_DISPLAY_NAME;
  return {
    displayName,
    avatarEmoji: sanitizeAvatarEmoji(profile.avatar_emoji),
  };
}

export function normalizePromptProfile(
  profile: Partial<PromptProfileDto> | null | undefined,
): PromptProfileDto {
  return {
    display_name:
      sanitizeDisplayName(profile?.display_name ?? "") || DEFAULT_DISPLAY_NAME,
    avatar_emoji: sanitizeAvatarEmoji(profile?.avatar_emoji ?? null),
    persona: profile?.persona?.trim() ?? "",
    writing_style: profile?.writing_style?.trim() ?? "",
    custom_rules: (profile?.custom_rules ?? [])
      .map((rule) => rule.trim())
      .filter(Boolean),
    language: profile?.language?.trim() || "zh-CN",
  };
}

interface LegacyAssistantIdentity {
  displayName?: string;
  avatarEmoji?: string | null;
}

function loadLegacyAssistantIdentity(): LegacyAssistantIdentity | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(LEGACY_IDENTITY_STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as LegacyAssistantIdentity;
  } catch {
    return null;
  }
}

function clearLegacyAssistantIdentity(): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(LEGACY_IDENTITY_STORAGE_KEY);
}

/** 若存在旧 localStorage 身份且 profile 仍为默认展示名，则合并并清除 legacy。 */
export function mergeLegacyAssistantIdentity(profile: PromptProfileDto): {
  profile: PromptProfileDto;
  migrated: boolean;
} {
  const legacy = loadLegacyAssistantIdentity();
  if (!legacy) {
    return { profile, migrated: false };
  }

  const legacyName = sanitizeDisplayName(legacy.displayName ?? "");
  const legacyEmoji = sanitizeAvatarEmoji(legacy.avatarEmoji ?? null);
  const isDefaultDisplay =
    sanitizeDisplayName(profile.display_name) === DEFAULT_DISPLAY_NAME;

  if (!isDefaultDisplay && !legacyName && !legacyEmoji) {
    clearLegacyAssistantIdentity();
    return { profile, migrated: false };
  }

  const next = normalizePromptProfile({
    ...profile,
    display_name:
      isDefaultDisplay && legacyName ? legacyName : profile.display_name,
    avatar_emoji:
      profile.avatar_emoji == null && legacyEmoji
        ? legacyEmoji
        : profile.avatar_emoji,
  });

  clearLegacyAssistantIdentity();
  return { profile: next, migrated: true };
}

export function dispatchPromptProfileChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(PROMPT_PROFILE_CHANGED));
}
