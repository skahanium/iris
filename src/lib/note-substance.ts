import { splitFrontmatter, titleFromFields } from "@/lib/frontmatter";
import { stripLeadingBodyTitleHeading } from "@/lib/markdown";

/** Placeholder titles that do not count as user-authored content. */
const PLACEHOLDER_TITLES = new Set(["", "无标题", "新建文档"]);

function isPlaceholderTitle(title: string): boolean {
  if (PLACEHOLDER_TITLES.has(title)) {
    return true;
  }
  if (/^无标题\d+$/.test(title)) {
    return true;
  }
  if (/^新建文档（\d+）$/.test(title)) {
    return true;
  }
  return false;
}

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
  if (!isPlaceholderTitle(title)) {
    return false;
  }
  if (!title) {
    const legacy = /^#\s+(.+?)\s*(?:\n|$)/.exec(rawBody.trimStart());
    if (legacy && isPlaceholderTitle(legacy[1]!.trim())) {
      return !bodyHasSubstance(
        rawBody.trimStart().slice(legacy[0].length).trimStart(),
      );
    }
  }
  const body = stripLeadingBodyTitleHeading(rawBody, title).trim();
  return !bodyHasSubstance(body);
}
