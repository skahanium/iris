export const NOTE_TITLE_SOFT_LIMIT = 80;
export const NOTE_TITLE_HARD_LIMIT = 200;

/** Collapse whitespace and strip line breaks for document title input. */
export function sanitizeDocumentTitleInput(raw: string): string {
  return raw.replace(/\r?\n/g, " ").replace(/\s+/g, " ").trimStart();
}
