/** 侧栏助手展示身份（本地偏好，不进 SQLite） */

export interface AssistantIdentity {
  /** 显示名称，默认 Iris */
  displayName: string;
  /** 单个 emoji；留空则用名称首字 */
  avatarEmoji: string | null;
}

export const DEFAULT_ASSISTANT_IDENTITY: AssistantIdentity = {
  displayName: "Iris",
  avatarEmoji: null,
};

const STORAGE_KEY = "iris-assistant-identity";
const MAX_NAME_LENGTH = 24;

/** 从存储读取；损坏或空名称时回退默认 */
export function loadAssistantIdentity(): AssistantIdentity {
  if (typeof localStorage === "undefined") {
    return { ...DEFAULT_ASSISTANT_IDENTITY };
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_ASSISTANT_IDENTITY };
    const parsed = JSON.parse(raw) as Partial<AssistantIdentity>;
    const displayName = sanitizeDisplayName(parsed.displayName ?? "");
    const avatarEmoji = sanitizeAvatarEmoji(parsed.avatarEmoji ?? null);
    return {
      displayName: displayName || DEFAULT_ASSISTANT_IDENTITY.displayName,
      avatarEmoji,
    };
  } catch {
    return { ...DEFAULT_ASSISTANT_IDENTITY };
  }
}

export function saveAssistantIdentity(identity: AssistantIdentity): void {
  const normalized: AssistantIdentity = {
    displayName:
      sanitizeDisplayName(identity.displayName) ||
      DEFAULT_ASSISTANT_IDENTITY.displayName,
    avatarEmoji: sanitizeAvatarEmoji(identity.avatarEmoji),
  };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(normalized));
}

export const ASSISTANT_IDENTITY_CHANGED = "iris-assistant-identity-changed";

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
  if (!trimmed) return "I";
  return [...trimmed][0]?.toUpperCase() ?? "I";
}
