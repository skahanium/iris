import { useCallback, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  Copy,
  FileText,
  Layers,
  ListChecks,
  ShieldCheck,
  X,
} from "lucide-react";

import { CitationCheckView } from "@/components/ai/CitationCheckView";
import { MarkdownRenderable } from "@/components/ai/MarkdownRenderable";
import { DiffView } from "@/components/ai/PatchPreview";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  organizeApply as defaultOrganizeApply,
  patchApply as defaultPatchApply,
} from "@/lib/ipc";
import { utf8ByteRangeToStringRange } from "@/lib/utf8-range";
import type {
  CitationCheckResult,
  OrganizeSuggestion,
  PatchApplyResult,
  PatchProposal,
  ResearchFocusPayload,
} from "@/types/ai";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type {
  AgentTaskDto,
  AgentTaskEventDto,
  AgentTaskStepDto,
} from "@/types/ipc";

interface ArtifactWorkspaceViewProps {
  tab: ArtifactTab;
  getNoteContent: () => string;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
}

interface ProcessArtifactPayload {
  task?: AgentTaskDto | null;
  steps?: AgentTaskStepDto[];
  events?: AgentTaskEventDto[];
  plan?: string[];
  evidenceGaps?: string[];
  verificationFailures?: string[];
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function asResearchPayload(value: unknown): ResearchFocusPayload | null {
  const record = asRecord(value);
  return typeof record.topic === "string" &&
    typeof record.summary === "string" &&
    typeof record.evidence_matrix === "object"
    ? (value as ResearchFocusPayload)
    : null;
}

function patchPayload(value: unknown): PatchProposal[] {
  const record = asRecord(value);
  const patches = record.patches;
  return Array.isArray(patches) ? (patches as PatchProposal[]) : [];
}

function citationPayload(value: unknown): CitationCheckResult | null {
  const record = asRecord(value);
  const result = record.result ?? value;
  return asRecord(result).coverage ? (result as CitationCheckResult) : null;
}

function organizePayload(value: unknown): OrganizeSuggestion[] {
  const record = asRecord(value);
  const suggestions = record.suggestions;
  return Array.isArray(suggestions)
    ? (suggestions as OrganizeSuggestion[])
    : [];
}

function processPayload(value: unknown): ProcessArtifactPayload {
  return asRecord(value) as ProcessArtifactPayload;
}

function ArtifactHeader({ tab }: { tab: ArtifactTab }) {
  return (
    <header className="border-b border-border/60 px-6 py-4">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs text-muted-foreground">AI 临时视图</p>
          <h1 className="truncate text-xl font-semibold text-foreground">
            {tab.title}
          </h1>
        </div>
        <Badge variant="outline">只读</Badge>
      </div>
    </header>
  );
}

function EvidenceSourcesArtifactView({ tab }: { tab: ArtifactTab }) {
  const result = asResearchPayload(tab.payload);
  if (!result) {
    return <p className="text-sm text-muted-foreground">暂无研究结果。</p>;
  }
  const propositions = result.evidence_matrix.propositions;
  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <ShieldCheck className="h-4 w-4" />
            研究综述
          </CardTitle>
        </CardHeader>
        <CardContent>
          <MarkdownRenderable
            content={result.summary}
            profile="chat_assistant"
            streaming={false}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Layers className="h-4 w-4" />
            证据矩阵
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid grid-cols-3 gap-2 text-center">
            <div>
              <div className="text-lg font-semibold">{propositions.length}</div>
              <div className="text-xs text-muted-foreground">子命题</div>
            </div>
            <div>
              <div className="text-lg font-semibold">
                {result.evidence_matrix.total_evidence_count}
              </div>
              <div className="text-xs text-muted-foreground">证据条目</div>
            </div>
            <div>
              <div className="text-lg font-semibold">
                {Math.round(result.evidence_matrix.coverage_score * 100)}%
              </div>
              <div className="text-xs text-muted-foreground">覆盖率</div>
            </div>
          </div>
          {result.evidence_matrix.global_gaps.length ? (
            <div className="rounded-md border border-border/60 px-3 py-2">
              <p className="text-sm font-medium">证据缺口</p>
              {result.evidence_matrix.global_gaps.map((gap) => (
                <p key={gap} className="mt-1 text-sm text-muted-foreground">
                  {gap}
                </p>
              ))}
            </div>
          ) : null}
          {propositions.map((item) => (
            <div
              key={item.id}
              className="rounded-md border border-border/60 p-3"
            >
              <div className="flex items-start gap-2">
                <Badge variant="outline">{item.id}</Badge>
                <p className="text-sm">{item.statement}</p>
              </div>
              {item.gaps.length ? (
                <p className="mt-2 text-xs text-muted-foreground">
                  缺口：{item.gaps.join(" / ")}
                </p>
              ) : null}
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function TaskProcessArtifactView({ tab }: { tab: ArtifactTab }) {
  const payload = processPayload(tab.payload);
  const task = payload.task ?? null;
  const plan = payload.plan ?? task?.deliberation_state?.plan_outline ?? [];
  const gaps =
    payload.evidenceGaps ?? task?.deliberation_state?.evidence_gaps ?? [];
  const failures =
    payload.verificationFailures ??
    task?.verification_summary?.items
      .filter((item) => item.status === "failed")
      .map((item) => item.description) ??
    [];
  const pauseReason =
    task?.status === "paused_budget" || task?.status === "paused_recoverable"
      ? task.error_message || "任务已暂停，等待继续。"
      : null;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <ListChecks className="h-4 w-4" />
            过程详情
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {plan.length ? (
            <section>
              <h2 className="text-sm font-medium">计划</h2>
              {plan.map((item) => (
                <p key={item} className="mt-1 text-sm text-muted-foreground">
                  {item}
                </p>
              ))}
            </section>
          ) : null}
          {gaps.length ? (
            <section>
              <h2 className="text-sm font-medium">证据缺口</h2>
              {gaps.map((item) => (
                <p key={item} className="mt-1 text-sm text-muted-foreground">
                  {item}
                </p>
              ))}
            </section>
          ) : null}
          {failures.length ? (
            <section>
              <h2 className="text-sm font-medium">验证未通过</h2>
              {failures.map((item) => (
                <p key={item} className="mt-1 text-sm text-muted-foreground">
                  {item}
                </p>
              ))}
            </section>
          ) : null}
          {pauseReason ? (
            <section>
              <h2 className="text-sm font-medium">暂停原因</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                {pauseReason}
              </p>
            </section>
          ) : null}
          {payload.steps?.length ? (
            <section>
              <h2 className="text-sm font-medium">步骤摘要</h2>
              {payload.steps.map((step) => (
                <p key={step.id} className="mt-1 text-sm text-muted-foreground">
                  {step.output_summary || step.input_summary}
                </p>
              ))}
            </section>
          ) : null}
          {payload.events?.length ? (
            <section>
              <h2 className="text-sm font-medium">事件</h2>
              {payload.events
                .filter((event) => event.message.trim().length > 0)
                .map((event) => (
                  <p
                    key={event.id}
                    className="mt-1 text-sm text-muted-foreground"
                  >
                    {event.message}
                  </p>
                ))}
            </section>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function WritingChangeArtifactView({
  tab,
  getNoteContent,
  onPatchApplied,
}: ArtifactWorkspaceViewProps) {
  const [patches, setPatches] = useState(() => patchPayload(tab.payload));
  const [error, setError] = useState<string | null>(null);

  const acceptPatch = useCallback(
    async (patch: PatchProposal) => {
      try {
        const result: PatchApplyResult = await defaultPatchApply(patch);
        if (!result.success) {
          throw new Error(result.error ?? "补丁应用失败");
        }
        const noteContent = getNoteContent();
        const range = utf8ByteRangeToStringRange(noteContent, patch.range);
        if (!range) {
          throw new Error("补丁范围不是有效的 UTF-8 字节边界");
        }
        onPatchApplied?.(
          noteContent.slice(0, range.start) +
            patch.replacement_text +
            noteContent.slice(range.end),
        );
        setPatches((prev) => prev.filter((item) => item.id !== patch.id));
      } catch (err) {
        setError(invokeErrorMessage(err));
      }
    },
    [getNoteContent, onPatchApplied],
  );

  const copyPatch = useCallback(async (patch: PatchProposal) => {
    try {
      await navigator.clipboard.writeText(patch.replacement_text);
    } catch {
      /* ignore */
    }
  }, []);

  return (
    <div className="space-y-3">
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      {patches.length ? (
        patches.map((patch) => (
          <Card key={patch.id} className="border-border/60">
            <CardHeader className="pb-2">
              <div className="flex items-center justify-between gap-3">
                <CardTitle className="text-sm font-medium">写作修改</CardTitle>
                <Badge variant="outline" className="text-xs">
                  {patch.evidence_packet_ids.length} 条证据
                </Badge>
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              {patch.warnings.length ? (
                <div className="space-y-1">
                  {patch.warnings.map((warning) => (
                    <div
                      key={warning}
                      className="flex items-start gap-2 rounded-md bg-yellow-500/5 px-2 py-1.5 text-xs text-yellow-600"
                    >
                      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                      <MarkdownRenderable
                        content={warning}
                        profile="patch_preview"
                        className="text-xs"
                      />
                    </div>
                  ))}
                </div>
              ) : null}
              <DiffView
                beforeText={patch.original_text}
                afterText={patch.replacement_text}
                patchType={
                  patch.range.start === patch.range.end ? "insert" : "replace"
                }
                riskLevel={patch.risk_level}
                targetPath={patch.target_path}
              />
              <div className="flex items-center justify-between pt-1">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void copyPatch(patch)}
                  title="复制替换文本"
                >
                  <Copy className="mr-1 h-3.5 w-3.5" />
                  复制
                </Button>
                <div className="flex gap-1.5">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      setPatches((prev) =>
                        prev.filter((item) => item.id !== patch.id),
                      )
                    }
                    title="拒绝修改"
                  >
                    <X className="mr-1 h-3.5 w-3.5" />
                    拒绝
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void acceptPatch(patch)}
                    title="接受修改"
                  >
                    <Check className="mr-1 h-3.5 w-3.5" />
                    接受
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        ))
      ) : (
        <p className="text-sm text-muted-foreground">暂无待处理修改。</p>
      )}
    </div>
  );
}

function CitationStructuredResultView({ tab }: { tab: ArtifactTab }) {
  const result = citationPayload(tab.payload);
  return result ? (
    <CitationCheckView result={result} />
  ) : (
    <p className="text-sm text-muted-foreground">暂无引用检查结果。</p>
  );
}

function OrganizeStructuredResultView({
  tab,
  onVaultRefresh,
}: ArtifactWorkspaceViewProps) {
  const [suggestions, setSuggestions] = useState(() =>
    organizePayload(tab.payload),
  );
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(suggestions.map((item) => item.id)),
  );
  const [error, setError] = useState<string | null>(null);

  const selectedSuggestions = useMemo(
    () => suggestions.filter((item) => selected.has(item.id)),
    [selected, suggestions],
  );

  const applySelected = useCallback(async () => {
    if (!selectedSuggestions.length) return;
    try {
      const result = await defaultOrganizeApply(selectedSuggestions);
      setSuggestions((prev) =>
        prev.filter((item) => !result.applied.includes(item.id)),
      );
      setSelected(new Set());
      onVaultRefresh?.();
    } catch (err) {
      setError(invokeErrorMessage(err));
    }
  }, [onVaultRefresh, selectedSuggestions]);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <FileText className="h-4 w-4" />
          整理建议
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        {suggestions.map((item) => (
          <label
            key={item.id}
            className="flex items-start gap-2 rounded-md border border-border/60 p-3 text-sm"
          >
            <input
              type="checkbox"
              checked={selected.has(item.id)}
              onChange={() =>
                setSelected((prev) => {
                  const next = new Set(prev);
                  if (next.has(item.id)) next.delete(item.id);
                  else next.add(item.id);
                  return next;
                })
              }
              className="mt-1"
            />
            <span>
              <span className="font-medium">{item.target_path}</span>
              <span className="block text-muted-foreground">
                {item.reason}：{item.suggested_value}
              </span>
            </span>
          </label>
        ))}
        {suggestions.length ? (
          <Button type="button" onClick={() => void applySelected()}>
            应用已选
          </Button>
        ) : (
          <p className="text-sm text-muted-foreground">暂无整理建议。</p>
        )}
      </CardContent>
    </Card>
  );
}

function DocumentIssuesStructuredResultView({ tab }: { tab: ArtifactTab }) {
  const record = asRecord(tab.payload);
  const issues = Array.isArray(record.issues)
    ? (record.issues as string[])
    : [];
  const summary = typeof record.summary === "string" ? record.summary : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <FileText className="h-4 w-4" />
          文档问题清单
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        {summary ? (
          <MarkdownRenderable
            content={summary}
            profile="chat_assistant"
            streaming={false}
          />
        ) : null}
        {issues.length ? (
          <div className="space-y-2">
            {issues.map((issue) => (
              <p
                key={issue}
                className="rounded-md border border-border/60 px-3 py-2 text-sm text-muted-foreground"
              >
                {issue}
              </p>
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">暂无文档问题。</p>
        )}
      </CardContent>
    </Card>
  );
}

function StructuredResultArtifactView(props: ArtifactWorkspaceViewProps) {
  const record = asRecord(props.tab.payload);
  const resultKind = record.resultKind ?? record.schema;
  if (resultKind === "citation_report") {
    return <CitationStructuredResultView tab={props.tab} />;
  }
  if (resultKind === "organize_result") {
    return <OrganizeStructuredResultView {...props} />;
  }
  if (resultKind === "document_issues") {
    return <DocumentIssuesStructuredResultView tab={props.tab} />;
  }
  return <p className="text-sm text-muted-foreground">暂无结构化结果。</p>;
}

export function ArtifactWorkspaceView(props: ArtifactWorkspaceViewProps) {
  const { tab } = props;
  return (
    <div
      className="flex min-h-0 flex-1 flex-col bg-background"
      data-testid="artifact-workspace-view"
    >
      <ArtifactHeader tab={tab} />
      <main className="min-h-0 flex-1 overflow-auto px-6 py-5">
        <div className="mx-auto max-w-4xl">
          {tab.kind === "evidence_sources" ? (
            <EvidenceSourcesArtifactView tab={tab} />
          ) : null}
          {tab.kind === "task_process" ? (
            <TaskProcessArtifactView tab={tab} />
          ) : null}
          {tab.kind === "writing_change" ? (
            <WritingChangeArtifactView {...props} />
          ) : null}
          {tab.kind === "structured_result" ? (
            <StructuredResultArtifactView {...props} />
          ) : null}
        </div>
      </main>
    </div>
  );
}
