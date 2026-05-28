import { useCallback, useEffect, useRef, useState } from "react";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  FileText,
  GitBranch,
  Layers,
  Loader2,
  Search,
  Shield,
  StopCircle,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  researchAbort,
  researchExecute,
  researchGenerateNote,
  researchStatus,
  listenResearchProgress,
} from "@/lib/ipc";

// ─── Types ───────────────────────────────────────────────

interface SubProposition {
  id: string;
  statement: string;
  evidence: Array<{
    id: string;
    title: string;
    citation_label: string;
    score: number;
    trust_level: string;
  }>;
  gaps: string[];
}

interface ArgumentLink {
  from_proposition_id: string;
  to_proposition_id: string;
  link_type: string;
  strength: number;
  evidence_label?: string;
}

interface ArgumentChain {
  links: ArgumentLink[];
  has_contradictions: boolean;
  chain_strength: number;
}

interface EvidenceMatrix {
  topic: string;
  propositions: SubProposition[];
  global_gaps: string[];
  total_evidence_count: number;
  coverage_score: number;
}

interface ResearchResult {
  request_id: string;
  topic: string;
  rounds: number;
  evidence_matrix: EvidenceMatrix;
  argument_chain: ArgumentChain;
  summary: string;
  total_tokens: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

interface ResearchProgressData {
  request_id: string;
  topic: string;
  state: string;
  current_round: number;
  max_rounds: number;
  queries_executed: string[];
  new_evidence_count: number;
  total_evidence_count: number;
  tokens_used: number;
  token_budget: number;
  progress_pct: number;
  round_terminated_early: boolean;
}

// ─── Link Type Labels ────────────────────────────────────

const LINK_TYPE_LABELS: Record<string, { label: string; className: string }> = {
  supports: { label: "支持", className: "text-foreground" },
  contradicts: { label: "矛盾", className: "text-destructive" },
  prerequisite: { label: "前提", className: "text-muted-foreground" },
  consequence: { label: "推论", className: "text-muted-foreground" },
  parallel: { label: "并列", className: "text-muted-foreground" },
};

function coverageScoreClass(score: number): string {
  if (score < 0.5) return "text-muted-foreground";
  if (score < 0.8) return "text-foreground/80";
  return "text-foreground";
}

// ─── Component ───────────────────────────────────────────

interface ResearchPanelProps {
  notePath: string | null;
  /** 与底栏「联网」开关一致；勿在面板内重复授权 */
  webSearch?: boolean;
}

export function ResearchPanel({
  notePath: _notePath,
  webSearch = false,
}: ResearchPanelProps) {
  const [topic, setTopic] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<ResearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expandedProps, setExpandedProps] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<ResearchProgressData | null>(null);
  const [generatingNote, setGeneratingNote] = useState(false);

  const requestIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  // Load recent research on mount
  useEffect(() => {
    researchStatus()
      .then((status) => {
        // status loaded but not displayed
        void status;
      })
      .catch(() => {});
  }, []);

  // Listen to research progress events
  useEffect(() => {
    const setupListener = async () => {
      const unlisten = await listenResearchProgress((payload) => {
        setProgress(payload);

        const state = payload.state;
        if (state === "completed" || state === "failed" || state === "aborted") {
          setLoading(false);
        }
      });
      unlistenRef.current = unlisten;
    };
    void setupListener();

    return () => {
      unlistenRef.current?.();
    };
  }, []);

  const toggleProp = useCallback((id: string) => {
    setExpandedProps((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const executeResearch = useCallback(async () => {
    if (!topic.trim() || loading) return;

    setLoading(true);
    setError(null);
    setResult(null);
    setProgress(null);

    try {
      const res = await researchExecute({
        topic: topic.trim(),
        web_authorized: webSearch,
      });
      requestIdRef.current = res.request_id;
      setResult(res as unknown as ResearchResult);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [topic, loading, webSearch]);

  const handleAbort = useCallback(async () => {
    if (!requestIdRef.current) return;
    try {
      await researchAbort(requestIdRef.current);
      setLoading(false);
      setProgress((prev) =>
        prev
          ? { ...prev, state: "aborted" }
          : null,
      );
    } catch (e) {
      console.error("Abort failed:", e);
    }
  }, []);

  const handleGenerateNote = useCallback(async () => {
    if (!result) return;
    setGeneratingNote(true);
    try {
      const note = await researchGenerateNote({
        topic: result.topic,
        summary: result.summary,
        evidence_count: result.evidence_matrix.total_evidence_count,
        coverage_score: result.evidence_matrix.coverage_score,
      });
      // Alert user with the suggested path
      alert(
        `研究笔记已生成，建议路径: ${note.suggested_path}\n\n${note.content.substring(0, 200)}...`,
      );
    } catch (e) {
      console.error("Generate note failed:", e);
    } finally {
      setGeneratingNote(false);
    }
  }, [result]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-b border-border p-3">
        <div className="mb-2 flex items-center gap-2">
          <BookOpen className="h-4 w-4 text-primary" />
          <span className="text-sm font-medium">研究助理</span>
          <Badge variant="secondary" className="text-[10px]">
            L3 半自治
          </Badge>
        </div>

        <div className="flex gap-2">
          <input
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void executeResearch();
              }
            }}
            placeholder="输入研究主题或问题…"
            className="flex-1 rounded-md border border-input bg-background px-3 py-1.5 text-sm"
            disabled={loading}
          />
          {loading ? (
            <Button size="sm" variant="destructive" onClick={handleAbort}>
              <StopCircle className="h-4 w-4" />
              中止
            </Button>
          ) : (
            <Button
              size="sm"
              disabled={!topic.trim()}
              onClick={() => void executeResearch()}
            >
              <Search className="h-4 w-4" />
              研究
            </Button>
          )}
        </div>

        {/* Progress bar */}
        {progress && loading && (
          <div className="mt-2 space-y-1">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>
                第 {progress.current_round}/{progress.max_rounds} 轮
                {progress.round_terminated_early && "（已充分）"}
              </span>
              <span>
                {progress.total_evidence_count} 条证据 |
                {progress.tokens_used.toLocaleString()} tokens
              </span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary transition-all duration-500"
                style={{
                  width: `${Math.round(progress.progress_pct * 100)}%`,
                }}
              />
            </div>
            {progress.queries_executed.length > 0 && (
              <div className="text-[10px] text-muted-foreground">
                {progress.queries_executed.slice(-2).join(" · ")}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Content */}
      <ScrollArea className="flex-1">
        {error && (
          <div className="m-3 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-xs text-destructive">
            {error}
          </div>
        )}

        {result && (
          <div className="space-y-3 p-3">
            {/* Action bar */}
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted-foreground">
                请求 ID: {result.request_id.substring(0, 8)}…
              </span>
              <Button
                size="sm"
                variant="outline"
                onClick={handleGenerateNote}
                disabled={generatingNote}
              >
                {generatingNote ? (
                  <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
                ) : (
                  <FileText className="mr-1 h-3.5 w-3.5" />
                )}
                生成研究笔记
              </Button>
            </div>

            {/* Coverage Summary */}
            <Card>
              <CardHeader className="p-3 pb-2">
                <CardTitle className="flex items-center gap-2 text-sm">
                  <Layers className="h-4 w-4" />
                  证据矩阵
                </CardTitle>
              </CardHeader>
              <CardContent className="p-3 pt-0">
                <div className="grid grid-cols-3 gap-2 text-center">
                  <div>
                    <div className="text-lg font-bold">
                      {result.evidence_matrix.propositions.length}
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      子命题
                    </div>
                  </div>
                  <div>
                    <div className="text-lg font-bold">
                      {result.evidence_matrix.total_evidence_count}
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      证据条目
                    </div>
                  </div>
                  <div>
                    <div
                      className={`text-lg font-bold tabular-nums ${coverageScoreClass(
                        result.evidence_matrix.coverage_score,
                      )}`}
                    >
                      {Math.round(result.evidence_matrix.coverage_score * 100)}%
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      覆盖率
                    </div>
                  </div>
                </div>

                {result.evidence_matrix.global_gaps.length > 0 && (
                  <div className="mt-2 rounded-md border border-border/80 bg-surface-inset p-2">
                    <p className="mb-1 text-[10px] font-medium text-muted-foreground">
                      证据缺口：
                    </p>
                    {result.evidence_matrix.global_gaps.map((gap, i) => (
                      <p key={i} className="text-xs text-muted-foreground">
                        · {gap}
                      </p>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>

            {/* Sub-propositions */}
            <Card>
              <CardHeader className="p-3 pb-2">
                <CardTitle className="flex items-center gap-2 text-sm">
                  <BookOpen className="h-4 w-4" />
                  子命题分析
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-2 p-3 pt-0">
                {result.evidence_matrix.propositions.map((prop) => (
                  <div
                    key={prop.id}
                    className="rounded-md border border-border"
                  >
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 p-2 text-left"
                      onClick={() => toggleProp(prop.id)}
                    >
                      {expandedProps.has(prop.id) ? (
                        <ChevronDown className="h-3 w-3 shrink-0" />
                      ) : (
                        <ChevronRight className="h-3 w-3 shrink-0" />
                      )}
                      <Badge variant="outline" className="text-[10px]">
                        {prop.id}
                      </Badge>
                      <span className="text-xs">{prop.statement}</span>
                      <span className="ml-auto text-[10px] text-muted-foreground">
                        {prop.evidence.length} 条证据
                      </span>
                    </button>

                    {expandedProps.has(prop.id) && (
                      <div className="space-y-1 border-t border-border p-2">
                        {prop.evidence.map((ev) => (
                          <div
                            key={ev.id}
                            className="flex items-center gap-2 text-xs"
                          >
                            <Badge variant="secondary" className="text-[10px]">
                              {ev.citation_label}
                            </Badge>
                            <span className="truncate">{ev.title}</span>
                            <span className="ml-auto text-muted-foreground">
                              {Math.round(ev.score * 100)}%
                            </span>
                          </div>
                        ))}
                        {prop.gaps.length > 0 && (
                          <div className="mt-1 text-[10px] text-muted-foreground">
                            {prop.gaps.map((g, i) => (
                              <span key={i}>· {g} </span>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                ))}
              </CardContent>
            </Card>

            {/* Argument Chain */}
            {result.argument_chain.links.length > 0 && (
              <Card>
                <CardHeader className="p-3 pb-2">
                  <CardTitle className="flex items-center gap-2 text-sm">
                    <GitBranch className="h-4 w-4" />
                    论证链
                    {result.argument_chain.has_contradictions && (
                      <Badge variant="destructive" className="text-[10px]">
                        存在矛盾
                      </Badge>
                    )}
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-1 p-3 pt-0">
                  {result.argument_chain.links.map((link, i) => {
                    const typeInfo = LINK_TYPE_LABELS[link.link_type] ?? {
                      label: link.link_type,
                      className: "text-muted-foreground",
                    };
                    return (
                      <div key={i} className="flex items-center gap-2 text-xs">
                        <Badge variant="outline" className="text-[10px]">
                          {link.from_proposition_id}
                        </Badge>
                        <span className={typeInfo.className}>
                          {typeInfo.label}
                        </span>
                        <span className="text-muted-foreground">→</span>
                        <Badge variant="outline" className="text-[10px]">
                          {link.to_proposition_id}
                        </Badge>
                        <span className="ml-auto text-muted-foreground">
                          {Math.round(link.strength * 100)}%
                        </span>
                      </div>
                    );
                  })}
                </CardContent>
              </Card>
            )}

            {/* Summary */}
            <Card>
              <CardHeader className="p-3 pb-2">
                <CardTitle className="flex items-center gap-2 text-sm">
                  <Shield className="h-4 w-4" />
                  研究综述
                </CardTitle>
              </CardHeader>
              <CardContent className="p-3 pt-0">
                <div className="whitespace-pre-wrap text-xs leading-relaxed">
                  {result.summary}
                </div>
              </CardContent>
            </Card>

            {/* Token Usage */}
            <div className="text-center text-[10px] text-muted-foreground">
              消耗 {result.total_tokens.total_tokens} tokens |{" "}
              {result.rounds} 轮检索
            </div>
          </div>
        )}

        {!result && !loading && !error && (
          <div className="flex h-full flex-col items-center justify-center text-muted-foreground">
            <BookOpen className="mb-2 h-8 w-8 opacity-30" />
            <p className="text-sm">输入研究主题开始</p>
            <p className="mt-1 text-xs">
              支持子命题拆解、证据矩阵、论证链分析
            </p>
            <p className="mt-1 text-xs text-muted-foreground/60">
              每轮可见进度 · 可随时中止
            </p>
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
