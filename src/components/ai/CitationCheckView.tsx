import { useState } from "react";
import {
  AlertTriangle,
  BookOpen,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  HelpCircle,
  XCircle,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { ContextPacket } from "@/types/ai";

// ─── Types ──────────────────────────────────────────────

interface FactClaim {
  id: string;
  statement: string;
  has_support: boolean;
  supporting_evidence: string[];
  conflicting_evidence: string[];
}

interface CitationSuggestion {
  claim_id: string;
  action: string;
  suggested_citation?: string;
  explanation: string;
}

interface CitationCheckResultData {
  request_id: string;
  claims: FactClaim[];
  coverage: string;
  suggestions: CitationSuggestion[];
  evidence_used: ContextPacket[];
}

// ─── Coverage Styling ───────────────────────────────────

const COVERAGE_CONFIG: Record<
  string,
  { label: string; icon: typeof CheckCircle; className: string }
> = {
  well_supported: {
    label: "充分支持",
    icon: CheckCircle,
    className: "text-green-600 bg-green-500/10",
  },
  partially_supported: {
    label: "部分支持",
    icon: HelpCircle,
    className: "text-yellow-600 bg-yellow-500/10",
  },
  weakly_supported: {
    label: "支持不足",
    icon: AlertTriangle,
    className: "text-orange-600 bg-orange-500/10",
  },
  unsupported: {
    label: "无依据",
    icon: XCircle,
    className: "text-red-600 bg-red-500/10",
  },
  contradicted: {
    label: "存在冲突",
    icon: XCircle,
    className: "text-red-600 bg-red-500/10",
  },
};

const ACTION_LABELS: Record<string, string> = {
  add_citation: "添加引用",
  rewrite: "改写",
  remove_claim: "删除声明",
  add_qualifier: "添加限定词",
};

// ─── Component ──────────────────────────────────────────

interface CitationCheckViewProps {
  result: CitationCheckResultData;
  onApplySuggestion?: (suggestion: CitationSuggestion) => void;
}

export function CitationCheckView({
  result,
  onApplySuggestion,
}: CitationCheckViewProps) {
  const [expandedClaims, setExpandedClaims] = useState<Set<string>>(new Set());

  const coverageConfig =
    COVERAGE_CONFIG[result.coverage] ?? COVERAGE_CONFIG.unsupported!;
  const CoverageIcon = coverageConfig.icon;

  const toggleClaim = (claimId: string) => {
    setExpandedClaims((prev) => {
      const next = new Set(prev);
      if (next.has(claimId)) {
        next.delete(claimId);
      } else {
        next.add(claimId);
      }
      return next;
    });
  };

  // Find evidence packet by ID
  const findPacket = (id: string) =>
    result.evidence_used.find((p) => p.id === id);

  // Find suggestion for a claim
  const findSuggestion = (claimId: string) =>
    result.suggestions.find((s) => s.claim_id === claimId);

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-medium">引用检查结果</CardTitle>
          <Badge
            variant="outline"
            className={cn("text-xs", coverageConfig.className)}
          >
            <CoverageIcon className="mr-1 h-3.5 w-3.5" />
            {coverageConfig.label}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {/* Claims */}
        {result.claims.length > 0 ? (
          <div className="space-y-2">
            {result.claims.map((claim) => {
              const isExpanded = expandedClaims.has(claim.id);
              const suggestion = findSuggestion(claim.id);
              const supportCount = claim.supporting_evidence.length;
              const conflictCount = claim.conflicting_evidence.length;

              return (
                <div
                  key={claim.id}
                  className="overflow-hidden rounded-md border border-border/60"
                >
                  {/* Claim header */}
                  <button
                    type="button"
                    className="flex w-full items-start gap-2 bg-muted/30 px-3 py-2 text-left text-xs hover:bg-muted/50"
                    onClick={() => toggleClaim(claim.id)}
                  >
                    {isExpanded ? (
                      <ChevronDown className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                    ) : (
                      <ChevronRight className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                    )}
                    <div className="min-w-0 flex-1">
                      <span className="line-clamp-2">{claim.statement}</span>
                    </div>
                    <div className="flex shrink-0 gap-1">
                      {supportCount > 0 && (
                        <Badge
                          variant="outline"
                          className="bg-green-500/10 text-green-600"
                        >
                          {supportCount} 支持
                        </Badge>
                      )}
                      {conflictCount > 0 && (
                        <Badge
                          variant="outline"
                          className="bg-red-500/10 text-red-600"
                        >
                          {conflictCount} 冲突
                        </Badge>
                      )}
                      {supportCount === 0 && conflictCount === 0 && (
                        <Badge
                          variant="outline"
                          className="bg-gray-500/10 text-gray-600"
                        >
                          无证据
                        </Badge>
                      )}
                    </div>
                  </button>

                  {/* Expanded details */}
                  {isExpanded && (
                    <div className="space-y-2 border-t border-border/40 px-3 py-2">
                      {/* Supporting evidence */}
                      {claim.supporting_evidence.length > 0 && (
                        <div>
                          <div className="mb-1 text-xs font-medium text-green-600">
                            支持证据
                          </div>
                          <div className="space-y-1">
                            {claim.supporting_evidence.map((packetId) => {
                              const packet = findPacket(packetId);
                              return packet ? (
                                <div
                                  key={packetId}
                                  className="rounded bg-green-500/5 px-2 py-1 text-xs"
                                >
                                  <span className="font-medium">
                                    {packet.citation_label}
                                  </span>
                                  <span className="ml-1 text-muted-foreground">
                                    {packet.title}
                                  </span>
                                </div>
                              ) : null;
                            })}
                          </div>
                        </div>
                      )}

                      {/* Conflicting evidence */}
                      {claim.conflicting_evidence.length > 0 && (
                        <div>
                          <div className="mb-1 text-xs font-medium text-red-600">
                            冲突证据
                          </div>
                          <div className="space-y-1">
                            {claim.conflicting_evidence.map((packetId) => {
                              const packet = findPacket(packetId);
                              return packet ? (
                                <div
                                  key={packetId}
                                  className="rounded bg-red-500/5 px-2 py-1 text-xs"
                                >
                                  <span className="font-medium">
                                    {packet.citation_label}
                                  </span>
                                  <span className="ml-1 text-muted-foreground">
                                    {packet.title}
                                  </span>
                                </div>
                              ) : null;
                            })}
                          </div>
                        </div>
                      )}

                      {/* Suggestion */}
                      {suggestion && (
                        <div className="flex items-start gap-2 rounded bg-blue-500/5 px-2 py-1.5">
                          <BookOpen className="mt-0.5 h-3.5 w-3.5 shrink-0 text-blue-600" />
                          <div className="min-w-0 flex-1">
                            <div className="text-xs font-medium text-blue-600">
                              建议：
                              {ACTION_LABELS[suggestion.action] ??
                                suggestion.action}
                            </div>
                            <div className="text-xs text-muted-foreground">
                              {suggestion.explanation}
                            </div>
                          </div>
                          {onApplySuggestion && (
                            <button
                              type="button"
                              className="shrink-0 rounded px-1.5 py-0.5 text-xs text-blue-600 hover:bg-blue-500/10"
                              onClick={() => onApplySuggestion(suggestion)}
                            >
                              应用
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="py-4 text-center text-xs text-muted-foreground">
            未检测到事实声明
          </div>
        )}

        {/* Summary */}
        <div className="flex items-center justify-between border-t border-border/40 pt-2 text-xs text-muted-foreground">
          <span>检测到 {result.claims.length} 个声明</span>
          <span>
            {result.claims.filter((c) => c.has_support).length} 有依据
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
