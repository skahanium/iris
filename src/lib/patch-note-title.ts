import {
  serializeNoteMarkdown,
  splitFrontmatter,
  titleFromFields,
} from "@/lib/frontmatter";

/**
 * 将 noteTitle 的编辑同步进内存中的 markdown（仅更新 frontmatter.title，不动正文）。
 * 避免编辑器因重挂载从陈旧 frontmatter 恢复旧标题。
 */
export function patchNoteTitleInMarkdown(md: string, title: string): string {
  const { yaml, fields, body } = splitFrontmatter(md);
  const nextTitle = title.trim();
  const prevTitle = titleFromFields(fields);

  if (prevTitle === nextTitle) {
    return md;
  }

  return serializeNoteMarkdown(yaml, nextTitle, body);
}
