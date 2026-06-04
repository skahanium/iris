import type { VersionEntry, VersionKind } from "@/types/ipc";

export type DayBucket = "today" | "yesterday" | "earlier";

export type CollapsedGroupType = "auto_idle" | "pre_restore";

export interface CollapsedVersionGroup {
  type: CollapsedGroupType;
  label: string;
  groupKey: string;
  entries: VersionEntry[];
}

export interface DayVersionSection {
  bucket: DayBucket;
  title: string;
  visible: VersionEntry[];
  collapsed: CollapsedVersionGroup[];
}

export interface GroupedVersionTimeline {
  finalized: VersionEntry[];
  days: DayVersionSection[];
  isEmpty: boolean;
}

const DAY_ORDER: DayBucket[] = ["today", "yesterday", "earlier"];

const DAY_TITLES: Record<DayBucket, string> = {
  today: "今天",
  yesterday: "昨天",
  earlier: "更早",
};

export function isFinalizedEntry(v: VersionEntry): boolean {
  return v.is_finalized || v.kind === "finalize";
}

export function collapsedGroupType(
  kind: VersionKind,
): CollapsedGroupType | null {
  if (kind === "auto_idle") return "auto_idle";
  if (kind === "pre_restore") return "pre_restore";
  return null;
}

export function isVisibleInDayList(v: VersionEntry): boolean {
  if (isFinalizedEntry(v)) return false;
  return collapsedGroupType(v.kind) === null;
}

/** Parse snapshot time from `created_at` or `version_no`. */
export function parseVersionDate(v: VersionEntry): Date {
  if (v.created_at) {
    const parsed = new Date(v.created_at);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  const ts = v.version_no;
  if (ts.length >= 14) {
    const iso = `${ts.slice(0, 4)}-${ts.slice(4, 6)}-${ts.slice(6, 8)}T${ts.slice(8, 10)}:${ts.slice(10, 12)}:${ts.slice(12, 14)}Z`;
    const parsed = new Date(iso);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date(0);
}

function startOfLocalDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

export function getDayBucket(date: Date, now: Date): DayBucket {
  const diff = startOfLocalDay(now) - startOfLocalDay(date);
  const oneDay = 86_400_000;
  if (diff <= 0) return "today";
  if (diff <= oneDay) return "yesterday";
  return "earlier";
}

/** @deprecated Prefer {@link formatVersionDisplayTime} for list rows. */
export function formatVersionTime(versionNo: string): string {
  if (versionNo.length >= 14) {
    return `${versionNo.slice(0, 4)}-${versionNo.slice(4, 6)}-${versionNo.slice(6, 8)} ${versionNo.slice(8, 10)}:${versionNo.slice(10, 12)}:${versionNo.slice(12, 14)}`;
  }
  return versionNo;
}

const VERSION_DISPLAY_TIME: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
};

/** Localized snapshot time from `created_at` (falls back to UTC `version_no`). */
export function formatVersionDisplayTime(v: VersionEntry): string {
  const d = parseVersionDate(v);
  if (Number.isNaN(d.getTime()) || d.getTime() === 0) {
    return formatVersionTime(v.version_no);
  }
  return d.toLocaleString("zh-CN", VERSION_DISPLAY_TIME);
}

export function kindLabel(kind: VersionKind): string {
  switch (kind) {
    case "finalize":
      return "定稿";
    case "manual":
      return "手动";
    case "auto_idle":
      return "自动";
    case "pre_restore":
      return "恢复前";
    case "pre_close":
      return "关闭前";
    default:
      return kind;
  }
}

function sortNewestFirst(entries: VersionEntry[]): VersionEntry[] {
  return [...entries].sort((a, b) => {
    const ta = parseVersionDate(a).getTime();
    const tb = parseVersionDate(b).getTime();
    return tb - ta || b.id - a.id;
  });
}

function buildCollapsedGroup(
  bucket: DayBucket,
  type: CollapsedGroupType,
  entries: VersionEntry[],
): CollapsedVersionGroup | null {
  if (entries.length === 0) return null;
  const label =
    type === "auto_idle"
      ? `自动备份（${entries.length}）`
      : `恢复相关（${entries.length}）`;
  return {
    type,
    label,
    groupKey: `${bucket}:${type}`,
    entries: sortNewestFirst(entries),
  };
}

function buildDaySection(
  bucket: DayBucket,
  entries: VersionEntry[],
): DayVersionSection {
  const visible = sortNewestFirst(entries.filter(isVisibleInDayList));
  const autoIdle = entries.filter((v) => v.kind === "auto_idle");
  const preRestore = entries.filter((v) => v.kind === "pre_restore");
  const collapsed = [
    buildCollapsedGroup(bucket, "auto_idle", autoIdle),
    buildCollapsedGroup(bucket, "pre_restore", preRestore),
  ].filter((g): g is CollapsedVersionGroup => g !== null);

  return {
    bucket,
    title: DAY_TITLES[bucket],
    visible,
    collapsed,
  };
}

export function groupVersions(
  versions: VersionEntry[],
  now: Date = new Date(),
): GroupedVersionTimeline {
  const finalized = sortNewestFirst(versions.filter(isFinalizedEntry));
  const rest = versions.filter((v) => !isFinalizedEntry(v));

  const byDay = new Map<DayBucket, VersionEntry[]>();
  for (const v of rest) {
    const bucket = getDayBucket(parseVersionDate(v), now);
    const list = byDay.get(bucket) ?? [];
    list.push(v);
    byDay.set(bucket, list);
  }

  const days = DAY_ORDER.filter((b) => byDay.has(b)).map((bucket) =>
    buildDaySection(bucket, byDay.get(bucket)!),
  );

  const isEmpty =
    finalized.length === 0 &&
    days.every((d) => d.visible.length === 0 && d.collapsed.length === 0);

  return { finalized, days, isEmpty };
}

export function isGroupExpanded(
  expandedKeys: ReadonlySet<string>,
  groupKey: string,
): boolean {
  return expandedKeys.has(groupKey);
}
