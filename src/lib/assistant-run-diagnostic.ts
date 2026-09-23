import type {
  AttributionStatus,
  DiagnosticIssueClass,
  RecordCompleteness,
} from "@/types/ai";

export interface DiagnosticStatusLabel {
  symbol: string;
  text: string;
}

const ATTRIBUTION_LABELS: Record<AttributionStatus, DiagnosticStatusLabel> = {
  unattributed: { symbol: "○", text: "无法归因" },
  suspected: { symbol: "◐", text: "疑似" },
  confirmed: { symbol: "●", text: "已证实" },
  "multiple-causes": { symbol: "⊕", text: "多个独立原因" },
};

const COMPLETENESS_LABELS: Record<RecordCompleteness, DiagnosticStatusLabel> = {
  complete: { symbol: "■", text: "记录完整" },
  "missing-events": { symbol: "□", text: "事件缺失" },
  "broken-correlation": { symbol: "☒", text: "关联断裂" },
  "persist-failed": { symbol: "⚠", text: "审计写入失败" },
  "outcome-unknown": { symbol: "?", text: "结果未知" },
};

const ISSUE_CLASS_LABELS: Record<DiagnosticIssueClass, string> = {
  "internal-contract": "内部合同",
  "external-service": "外部服务",
  "tool-execution": "工具执行",
  "model-behavior": "模型行为",
  "expected-restriction": "预期限制",
  "diagnostic-gap": "诊断缺口",
};

export function attributionLabel(
  status: AttributionStatus,
): DiagnosticStatusLabel {
  return ATTRIBUTION_LABELS[status];
}

export function completenessLabel(
  completeness: RecordCompleteness,
): DiagnosticStatusLabel {
  return COMPLETENESS_LABELS[completeness];
}

export function issueClassLabel(issueClass: DiagnosticIssueClass): string {
  return ISSUE_CLASS_LABELS[issueClass];
}

export const DIAGNOSTIC_QUERY_FAILURE_HEADLINE =
  "诊断查询失败，结果不可用，不能当作空成功。";

export const AUDIT_HEALTH_FAILURE_HEADLINE =
  "审计通道写入失败，这是独立健康信号，不能当作没有问题。";
