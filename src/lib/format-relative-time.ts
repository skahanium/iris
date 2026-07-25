/** User-facing relative time for vault file lists and workspace empty cards. */
export function formatRelativeTime(iso: string, nowMs = Date.now()): string {
  const elapsed = nowMs - new Date(iso).getTime();
  if (!Number.isFinite(elapsed) || elapsed < 0) {
    return new Date(iso).toLocaleDateString();
  }
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "昨天";
  if (days < 7) return `${days} 天前`;
  return new Date(iso).toLocaleDateString();
}
