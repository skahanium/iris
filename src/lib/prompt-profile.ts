/** PromptProfile 前端工具与迁移（单一数据源：SQLite via IPC） */

import type { PromptBehaviorDto, PromptProfileDto } from "@/lib/ipc";

export const DEFAULT_DISPLAY_NAME = "砚";
export const AVATAR_IDS = [
  "iris",
  "orbit",
  "axis",
  "frame",
  "lens",
  "grid",
  "flow",
  "signal",
] as const;
export type AvatarId = (typeof AVATAR_IDS)[number];
export const DEFAULT_AVATAR_ID: AvatarId = "iris";
export const AVATAR_LABELS: Record<AvatarId, string> = {
  iris: "Iris 标记",
  orbit: "轨道",
  axis: "轴线",
  frame: "方框",
  lens: "透镜",
  grid: "网格",
  flow: "流线",
  signal: "信号",
};

export const DEFAULT_PROMPT_PROFILE: PromptProfileDto = {
  display_name: DEFAULT_DISPLAY_NAME,
  avatar_id: DEFAULT_AVATAR_ID,
  persona: "",
  writing_style: "",
  custom_rules: [],
  behavior: {
    initiative: "balanced",
    directness: "balanced",
    tone: "natural",
    challenge: "balanced",
  },
  language: "zh-CN",
};

const LEGACY_IDENTITY_STORAGE_KEY = "iris-assistant-identity";
const MAX_NAME_LENGTH = 24;

export const PROMPT_PROFILE_CHANGED = "iris-prompt-profile-changed";

export interface AvatarIdentity {
  displayName: string;
  avatarId: AvatarId;
}

export function sanitizeDisplayName(value: string): string {
  return value.trim().slice(0, MAX_NAME_LENGTH);
}

export function normalizeAvatarId(value: unknown): AvatarId {
  return typeof value === "string" && AVATAR_IDS.includes(value as AvatarId)
    ? (value as AvatarId)
    : DEFAULT_AVATAR_ID;
}

export function avatarLabel(avatarId: AvatarId): string {
  return AVATAR_LABELS[avatarId];
}

export function profileToAvatarIdentity(
  profile: PromptProfileDto,
): AvatarIdentity {
  const displayName =
    sanitizeDisplayName(profile.display_name) || DEFAULT_DISPLAY_NAME;
  return {
    displayName,
    avatarId: normalizeAvatarId(profile.avatar_id),
  };
}

export function normalizePromptProfile(
  profile: Partial<PromptProfileDto> | null | undefined,
): PromptProfileDto {
  const legacyProfile = profile as
    | (Partial<PromptProfileDto> & {
        avatar_emoji?: unknown;
      })
    | null
    | undefined;
  return {
    display_name:
      sanitizeDisplayName(profile?.display_name ?? "") || DEFAULT_DISPLAY_NAME,
    avatar_id: normalizeAvatarId(
      profile?.avatar_id ?? legacyProfile?.avatar_emoji,
    ),
    persona: profile?.persona?.trim() ?? "",
    writing_style: profile?.writing_style?.trim() ?? "",
    custom_rules: (profile?.custom_rules ?? [])
      .map((rule) => rule.trim())
      .filter(Boolean),
    behavior: normalizeBehavior(profile?.behavior),
    language: profile?.language?.trim() || "zh-CN",
  };
}

function normalizeBehavior(
  value: Partial<PromptBehaviorDto> | undefined,
): PromptBehaviorDto {
  const fallback = DEFAULT_PROMPT_PROFILE.behavior;
  return {
    initiative:
      value?.initiative === "reactive" || value?.initiative === "proactive"
        ? value.initiative
        : fallback.initiative,
    directness:
      value?.directness === "concise" || value?.directness === "deliberate"
        ? value.directness
        : fallback.directness,
    tone:
      value?.tone === "reserved" || value?.tone === "warm"
        ? value.tone
        : fallback.tone,
    challenge:
      value?.challenge === "supportive" || value?.challenge === "critical"
        ? value.challenge
        : fallback.challenge,
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

/** Clear legacy storage only after the normalized profile has reached SQLite. */
export function clearPersistedLegacyAssistantIdentity(): void {
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
  const hasLegacyEmoji = Boolean(legacy.avatarEmoji?.trim());
  const isDefaultDisplay =
    sanitizeDisplayName(profile.display_name) === DEFAULT_DISPLAY_NAME;

  if (!isDefaultDisplay && !legacyName && !hasLegacyEmoji) {
    return { profile, migrated: false };
  }

  const next = normalizePromptProfile({
    ...profile,
    display_name:
      isDefaultDisplay && legacyName ? legacyName : profile.display_name,
    avatar_id: normalizeAvatarId(profile.avatar_id),
  });

  return { profile: next, migrated: true };
}

export function dispatchPromptProfileChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(PROMPT_PROFILE_CHANGED));
}
