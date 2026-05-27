/** Format ISO timestamps for recycle bin UI (zh-CN locale). */
export function formatRecycleTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Whole days until permanent purge (minimum 0). */
export function recycleDaysRemaining(expiresAtIso: string, now = Date.now()): number {
  const expires = new Date(expiresAtIso).getTime();
  if (Number.isNaN(expires)) {
    return 0;
  }
  const ms = expires - now;
  return Math.max(0, Math.ceil(ms / (24 * 60 * 60 * 1000)));
}

export function recycleRetentionLabel(daysRemaining: number): string {
  if (daysRemaining <= 0) {
    return "即将永久删除";
  }
  if (daysRemaining === 1) {
    return "剩余 1 天";
  }
  return `剩余 ${daysRemaining} 天`;
}
