const PLACEHOLDER_RE = /^(新建文档|无标题|untitled)/i;

export function isPlaceholderTitle(title: string): boolean {
  const t = title.trim();
  return !t || PLACEHOLDER_RE.test(t);
}
