/**
 * Defense-in-depth filters for assistant body text shown in the chat UI.
 *
 * The Rust stream sanitizer remains authoritative. This module only strips
 * known reasoning tags and incomplete open tags so a leaked fragment cannot
 * linger in the transcript if presentation delivery races ahead of reset.
 */

const REASONING_TAG_RE =
  /<\s*(?:thinking|think|reasoning)\b[^>]*>[\s\S]*?(?:<\s*\/\s*(?:thinking|think|reasoning)\s*>|$)/gi;

const PARTIAL_OPEN_TAG_RE = /<\s*(?:thinking|think|reasoning)\b[^>]*$/i;

/** Strip known private-reasoning markup from visible assistant body text. */
export function sanitizeAssistantVisibleText(value: string): string {
  const withoutTags = value.replace(REASONING_TAG_RE, "");
  return withoutTags.replace(PARTIAL_OPEN_TAG_RE, "").trimStart();
}
