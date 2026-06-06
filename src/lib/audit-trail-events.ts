/** 从全局底栏打开 AI 侧栏工具审计抽屉 */
export const OPEN_AUDIT_TRAIL_EVENT = "iris-open-audit-trail";

export function dispatchOpenAuditTrail(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(OPEN_AUDIT_TRAIL_EVENT));
}
