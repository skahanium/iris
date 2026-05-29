import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  FileText,
  GitBranch,
  Layers,
  Loader2,
  Shield,
} from "lucide-react";
import { useCallback, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export interface ResearchFocusViewResult {
  request_id: string;
  topic: string;
  rounds: number;
  summary: string;
  evidence_matrix: {
    total_evidence_count: number;
    coverage_score: number;
    global_gaps: string[];
    propositions: Array<{
      id: string;
      statement: string;
      evidence: Array<{
        id: string;
        title: string;
        citation_label: string;
        score: number;
      }>;
      gaps: string[];
    }>;
  };
  argument_chain: {
    has_contradictions: boolean;
    chain_strength: number;
    links: Array<{
      from_proposition_id: string;
      to_proposition_id: string;
      link_type: string;
      strength: number;
    }>;
  };
  total_tokens: {
    total_tokens: number;
  };
}

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

interface ResearchFocusViewProps {
  result: ResearchFocusViewResult;
  generatingNote?: boolean;
  onGenerateNote?: () => void;
}

export function ResearchFocusView({
  result,
  generatingNote = false,
  onGenerateNote,
}: ResearchFocusViewProps) {
  const [expandedProps, setExpandedProps] = useState<Set<string>>(new Set());

  const toggleProp = useCallback((id: string) => {
    setExpandedProps((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">
          请求 {result.request_id.slice(0, 8)}…
        </span>
        {onGenerateNote ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={generatingNote}
            onClick={onGenerateNote}
          >
            {generatingNote ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : (
              <FileText className="mr-1 h-3.5 w-3.5" />
            )}
            生成研究笔记
          </Button>
        ) : null}
      </div>

      <Card className="border-border/60">
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
              <div className="text-[10px] text-muted-foreground">子命题</div>
            </div>
            <div>
              <div className="text-lg font-bold">
                {result.evidence_matrix.total_evidence_count}
              </div>
              <div className="text-[10px] text-muted-foreground">证据条目</div>
            </div>
            <div>
              <div
                className={`text-lg font-bold tabular-nums ${coverageScoreClass(
                  result.evidence_matrix.coverage_score,
                )}`}
              >
                {Math.round(result.evidence_matrix.coverage_score * 100)}%
              </div>
              <div className="text-[10px] text-muted-foreground">覆盖率</div>
            </div>
          </div>
          {result.evidence_matrix.global_gaps.length > 0 ? (
            <div className="mt-2 rounded-md border border-border/80 bg-surface-inset p-2">
              <p className="mb-1 text-[10px] font-medium text-muted-foreground">
                证据缺口
              </p>
              {result.evidence_matrix.global_gaps.map((gap) => (
                <p key={gap} className="text-xs text-muted-foreground">
                  · {gap}
                </p>
              ))}
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card className="border-border/60">
        <CardHeader className="p-3 pb-2">
          <CardTitle className="flex items-center gap-2 text-sm">
            <BookOpen className="h-4 w-4" />
            子命题分析
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 p-3 pt-0">
          {result.evidence_matrix.propositions.map((prop) => (
            <div key={prop.id} className="rounded-md border border-border/60">
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
                  {prop.evidence.length} 条
                </span>
              </button>
              {expandedProps.has(prop.id) ? (
                <div className="space-y-1 border-t border-border/60 p-2">
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
                </div>
              ) : null}
            </div>
          ))}
        </CardContent>
      </Card>

      {result.argument_chain.links.length > 0 ? (
        <Card className="border-border/60">
          <CardHeader className="p-3 pb-2">
            <CardTitle className="flex items-center gap-2 text-sm">
              <GitBranch className="h-4 w-4" />
              论证链
              {result.argument_chain.has_contradictions ? (
                <Badge variant="destructive" className="text-[10px]">
                  存在矛盾
                </Badge>
              ) : null}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 p-3 pt-0">
            {result.argument_chain.links.map((link, index) => {
              const typeInfo = LINK_TYPE_LABELS[link.link_type] ?? {
                label: link.link_type,
                className: "text-muted-foreground",
              };
              return (
                <div
                  key={`${link.from_proposition_id}-${index}`}
                  className="flex items-center gap-2 text-xs"
                >
                  <Badge variant="outline" className="text-[10px]">
                    {link.from_proposition_id}
                  </Badge>
                  <span className={typeInfo.className}>{typeInfo.label}</span>
                  <span className="text-muted-foreground">→</span>
                  <Badge variant="outline" className="text-[10px]">
                    {link.to_proposition_id}
                  </Badge>
                </div>
              );
            })}
          </CardContent>
        </Card>
      ) : null}

      <Card className="border-border/60">
        <CardHeader className="p-3 pb-2">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Shield className="h-4 w-4" />
            研究综述
          </CardTitle>
        </CardHeader>
        <CardContent className="p-3 pt-0">
          <p className="whitespace-pre-wrap text-xs leading-relaxed text-muted-foreground">
            {result.summary}
          </p>
        </CardContent>
      </Card>

      <p className="text-center text-[10px] text-muted-foreground">
        消耗 {result.total_tokens.total_tokens} tokens · {result.rounds} 轮检索
      </p>
    </div>
  );
}
