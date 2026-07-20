import { splitFrontmatter } from "@/lib/frontmatter";

function bodyHasSubstance(body: string): boolean {
  const withoutCodeBlocks = body.replace(/```[\s\S]*?```/g, "CODE_BLOCK");
  const stripped = withoutCodeBlocks
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/`[^`]+`/g, "CODE")
    .replace(/^#{1,6}\s*$/gm, "")
    .replace(/^[-*+]\s*$/gm, "")
    .replace(/^\d+\.\s*$/gm, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_~>|[\]()]/g, "")
    .trim();
  return stripped.length > 0;
}

/**
 * A blank note is defined solely by its Markdown body. The filename is the
 * document title, so a legacy `frontmatter.title` cannot make an otherwise
 * blank scratch note substantive.
 */
export function isNoteSubstantivelyEmpty(markdown: string): boolean {
  return !bodyHasSubstance(splitFrontmatter(markdown).body);
}
