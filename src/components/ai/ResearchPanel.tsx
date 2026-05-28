import { useCallback, useEffect, useState } from "react";
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  GitBranch,
  Layers,
  Loader2,
  Search,
  Shield,
  Webhook,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { researchExecute, researchStatus } from "@/lib/ipc";

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
}

export function ResearchPanel({ notePath: _notePath }: ResearchPanelProps) {
  const [topic, setTopic] = useState("");
  const [loading, setLoading] = useState(false);
  const [webAuthorized, setWebAuthorized] = useState(false);
  const [result, setResult] = useState<ResearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expandedProps, setExpandedProps] = useState<Set<string>>(new Set());
  const [, setRecentResearch] = useState<
    Array<{ request_id: string; status: string; created_at: string }>
  >([]);

  // Load recent research on mount
  useEffect(() => {
    researchStatus()
      .then((status) => {
        setRecentResearch(status.recent_research ?? []);
      })
      .catch(() => {});
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

    try {
      const res = await researchExecute({
        topic: topic.trim(),
        web_authorized: webAuthorized,
      });
      setResult(res as unknown as ResearchResult);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [topic, loading, webAuthorized]);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="border-b border-border p-3">
        <div className="mb-2 flex items-center gap-2">
          <BookOpen className="h-4 w-4 text-primary" />
          <span className="text-sm font-medium">研究助理</span>
          <Badge variant="secondary" className="text-[10px]">
            L3 有限循环
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
          <Button
            size="sm"
            disabled={loading || !topic.trim()}
            onClick={() => void executeResearch()}
          >
            {loading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Search className="h-4 w-4" />
            )}
            研究
          </Button>
        </div>

        <div className="mt-2 flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={webAuthorized}
              onChange={(e) => setWebAuthorized(e.target.checked)}
              className="h-3 w-3"
            />
            <Webhook className="h-3 w-3" />
            允许联网研究
          </label>
        </div>
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
              消耗 {result.total_tokens.total_tokens} tokens |{result.rounds}{" "}
              轮检索
            </div>
          </div>
        )}

        {!result && !loading && !error && (
          <div className="flex h-full flex-col items-center justify-center text-muted-foreground">
            <BookOpen className="mb-2 h-8 w-8 opacity-30" />
            <p className="text-sm">输入研究主题开始</p>
            <p className="mt-1 text-xs">支持子命题拆解、证据矩阵、论证链分析</p>
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
