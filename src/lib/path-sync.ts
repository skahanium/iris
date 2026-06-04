const PLACEHOLDER_RE = /^(未命名文档|新建文档|无标题|untitled)/i;

export function isPlaceholderTitle(title: string): boolean {
  const t = title.trim();
  if (!t) {
    return false;
  }
  return PLACEHOLDER_RE.test(t);
}
