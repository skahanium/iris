import { useCallback, useState } from "react";

import { invokeErrorMessage } from "@/lib/credentials";
import { assistantRunDiagnose } from "@/lib/ipc";
import {
  AUDIT_HEALTH_FAILURE_HEADLINE,
  DIAGNOSTIC_QUERY_FAILURE_HEADLINE,
  attributionLabel,
  completenessLabel,
  issueClassLabel,
} from "@/lib/assistant-run-diagnostic";
import type {
  AssistantSessionRef,
  DiagnosticFinding,
  DiagnosticReport,
} from "@/types/ai";

interface AssistantRunDiagnosticEntryProps {
  session: AssistantSessionRef;
  runId: string;
}

interface FindingListProps {
  title: string;
  items: DiagnosticFinding[];
}

function FindingList({ title, items }: FindingListProps) {
  if (items.length === 0) return null;
  return (
    <section className="space-y-1">
      <h4 className="text-[11px] font-medium text-foreground">{title}</h4>
      <ul className="space-y-1">
        {items.map((item, index) => (
          <li
            key={`${item.discovery.callId}:${item.discovery.attemptId}:${index}`}
            className="text-[11px] text-muted-foreground"
          >
            <span className="text-foreground/80">{item.statement}</span>
            <span className="ml-1 font-mono text-[10px] tabular-nums">
              {item.discovery.module} → {item.discovery.component} →{" "}
              {item.discovery.callId}/{item.discovery.attemptId}
              {item.discovery.toolInstance
                ? ` · ${item.discovery.toolInstance}`
                : ""}
              {" · "}
              {issueClassLabel(item.issueClass)}
              {item.confirmedSource ? ` · ${item.confirmedSource}` : ""}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Presentational C27 report. Query failure must not reuse this success shape. */
export function AssistantRunDiagnosticReport({
  report,
}: {
  report: DiagnosticReport;
}) {
  const attribution = attributionLabel(report.attributionStatus);
  const completeness = completenessLabel(report.recordCompleteness);
  return (
    <article
      className="mt-1 space-y-2 rounded-md border border-border-subtle bg-surface-inset/30 px-2 py-2 text-[11px] text-muted-foreground"
      data-testid="assistant-run-diagnostic-report"
    >
      {report.auditHealth.persistFailed ? (
        <p
          className="rounded border border-warning/30 bg-warning-bg px-2 py-1 text-warning-foreground"
          data-testid="assistant-run-audit-health"
          role="status"
        >
          {AUDIT_HEALTH_FAILURE_HEADLINE}
        </p>
      ) : null}
      <p className="font-medium text-foreground">{report.headline}</p>
      <p>{report.impact}</p>
      <p>恢复：{report.recoveryState}</p>
      <p>
        <span aria-label={`归因状态：${attribution.text}`}>
          {attribution.symbol} {attribution.text}
        </span>
        <span className="mx-2 text-border">·</span>
        <span aria-label={`记录完整性：${completeness.text}`}>
          {completeness.symbol} {completeness.text}
        </span>
      </p>
      {report.path.length > 0 ? (
        <p className="font-mono text-[10px] tabular-nums">
          路径：
          {report.path
            .map((step) => {
              const instance = step.toolInstance
                ? ` · ${step.toolInstance}`
                : "";
              return `${step.module} → ${step.component} → ${step.callId}/${step.attemptId}${instance}`;
            })
            .join("；")}
        </p>
      ) : null}
      <FindingList title="已知事实" items={report.knownFacts} />
      <FindingList title="直接失败" items={report.directFailures} />
      <FindingList title="恢复结果" items={report.recoveryResults} />
      <FindingList title="待证根因" items={report.pendingRootCauses} />
      <FindingList title="预期限制" items={report.expectedRestrictions} />
      <FindingList title="证据缺口" items={report.evidenceGaps} />
    </article>
  );
}

/** Load C27 for the current answer or failure without copying a Run ID. */
export function AssistantRunDiagnosticEntry({
  session,
  runId,
}: AssistantRunDiagnosticEntryProps) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [queryError, setQueryError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setOpen(true);
    setLoading(true);
    setQueryError(null);
    try {
      const next = await assistantRunDiagnose({ session, runId });
      setReport(next);
    } catch (error) {
      setReport(null);
      setQueryError(invokeErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [runId, session]);

  return (
    <div className="mt-1" data-testid="assistant-run-diagnostic-entry">
      <button
        type="button"
        className="text-[11px] text-foreground/80 underline-offset-2 hover:underline"
        data-testid="assistant-run-diagnose-open"
        onClick={() => {
          void load();
        }}
      >
        {loading ? "正在查询诊断…" : "查看诊断"}
      </button>
      {open && queryError ? (
        <p
          className="mt-1 rounded-md border border-destructive/30 bg-destructive/5 px-2 py-1.5 text-[11px] text-foreground"
          data-testid="assistant-run-diagnose-query-failed"
          role="alert"
        >
          {DIAGNOSTIC_QUERY_FAILURE_HEADLINE} {queryError}
        </p>
      ) : null}
      {open && report ? <AssistantRunDiagnosticReport report={report} /> : null}
    </div>
  );
}
