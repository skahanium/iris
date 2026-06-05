/**
 * Auto snapshot policy for Iris (P0 idle + P1 tab leave).
 *
 * ## Document size (`AUTO_LEAVE_SNAPSHOT_MAX_CHARS` = 12_000)
 *
 * | Trigger | Length gate | Rationale |
 * |---------|-------------|-----------|
 * | `tab_leave` (switch/close background tab) | Yes — skip when body > 12k | Avoid serialization + IPC on every tab switch for large notes |
 * | `auto_idle` (10 min after last L1 save) | **No** frontend gate | Uses already-persisted `lastSavedSnapshot` markdown; backend dedup/cooldown caps churn |
 * | `app_close` | N/A — never enqueue leave snapshots | Close path must not start version IPC; see `setAppClosing` on scheduler |
 *
 * Product intent: large notes may still get infrequent idle safety nets; they do not get
 * an extra snapshot on every tab switch. To disable all automatic leave snapshots, set
 * `ENABLE_TAB_LEAVE_AUTO_SNAPSHOT` to `false`.
 */
export const AUTO_LEAVE_SNAPSHOT_MAX_CHARS = 12_000;

/**
 * P1 tab-leave auto snapshots. Enabled after P0 idle path and close regression
 * validation; set to `false` to disable leave snapshots without touching call sites.
 */
export const ENABLE_TAB_LEAVE_AUTO_SNAPSHOT = true;

export type AutoSnapshotLeaveReason = "tab_leave" | "app_close";

interface ShouldEnqueueAutoSnapshotOnLeaveInput {
  reason: AutoSnapshotLeaveReason;
  markdownLength: number;
}

export function shouldEnqueueAutoSnapshotOnLeave({
  reason,
  markdownLength,
}: ShouldEnqueueAutoSnapshotOnLeaveInput): boolean {
  if (reason === "app_close") {
    return false;
  }
  if (reason === "tab_leave" && !ENABLE_TAB_LEAVE_AUTO_SNAPSHOT) {
    return false;
  }
  return markdownLength <= AUTO_LEAVE_SNAPSHOT_MAX_CHARS;
}
