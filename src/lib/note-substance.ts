import { splitFrontmatter, titleFromFields } from "@/lib/frontmatter";
import { stripLeadingBodyTitleHeading } from "@/lib/markdown";

/** Placeholder titles that do not count as user-authored content. */
const PLACEHOLDER_TITLES = new Set(["", "无标题", "新建文档"]);

function bodyHasSubstance(body: string): boolean {
  const stripped = body
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`[^`]+`/g, "")
    .replace(/^#{1,6}\s*$/gm, "")
    .replace(/^[-*+]\s*$/gm, "")
    .replace(/^\d+\.\s*$/gm, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_~>|[\]()]/g, "")
    .trim();
  return stripped.length > 0;
}

/**
 * True when the note has no user-authored title or body (blank slate).
 * Used to skip persistence and remove the file on tab close / switch.
 */
export function isNoteSubstantivelyEmpty(md: string): boolean {
  const { fields, body: rawBody } = splitFrontmatter(md);
  const title = titleFromFields(fields).trim();
  if (!PLACEHOLDER_TITLES.has(title)) {
    return false;
  }
  const body = stripLeadingBodyTitleHeading(rawBody, title).trim();
  return !bodyHasSubstance(body);
}
