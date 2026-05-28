import { useCallback, useEffect, useState } from "react";
import { FileText, Layers, Loader2, ListChecks } from "lucide-react";

import { PatchPreview } from "@/components/ai/PatchPreview";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { invokeErrorMessage } from "@/lib/credentials";
import { sha256Hex } from "@/lib/content-hash";
import {
  chapterWritingExecute,
  documentCheckExecute,
  parseDocumentChapters,
  patchApply,
} from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { ChapterInfo, DocumentCheckType, PatchProposal } from "@/types/ai";

type SubMode = "chapter" | "document";

const CHECK_TYPES: { id: DocumentCheckType; label: string }[] = [
  { id: "outline_check", label: "大纲检查" },
  { id: "citation_gap_check", label: "引用缺口" },
  { id: "style_consistency", label: "风格一致性" },
  { id: "cross_doc_reference", label: "跨文档引用" },
];

interface ChapterDocumentPanelProps {
  notePath: string | null;
  noteContent: string;
  webSearch?: boolean;
  onPatchApplied?: (newContent: string) => void;
}

export function ChapterDocumentPanel({
  notePath,
  noteContent,
  webSearch = false,
  onPatchApplied,
}: ChapterDocumentPanelProps) {
  const [mode, setMode] = useState<SubMode>("chapter");
  const [chapters, setChapters] = useState<ChapterInfo[]>([]);
  const [selectedChapter, setSelectedChapter] = useState<ChapterInfo | null>(
    null,
  );
  const [chapterGoal, setChapterGoal] = useState("");
  const [checkType, setCheckType] =
    useState<DocumentCheckType>("outline_check");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [patches, setPatches] = useState<PatchProposal[]>([]);
  const [docSummary, setDocSummary] = useState<string | null>(null);
  const [docIssues, setDocIssues] = useState<string[]>([]);

  useEffect(() => {
    if (!noteContent.trim()) {
      setChapters([]);
      setSelectedChapter(null);
      return;
    }
    void parseDocumentChapters(noteContent)
      .then((list) => {
        setChapters(list as ChapterInfo[]);
        setSelectedChapter((list[0] as ChapterInfo) ?? null);
      })
      .catch(() => setChapters([]));
  }, [noteContent]);

  const runChapter = useCallback(async () => {
    if (!notePath || !selectedChapter || !chapterGoal.trim()) return;
    setLoading(true);
    setError(null);
    setPatches([]);
    try {
      const hash = await sha256Hex(noteContent);
      const res = await chapterWritingExecute({
        target_path: notePath,
        base_content_hash: hash,
        chapter: selectedChapter,
        writing_goal: chapterGoal.trim(),
        web_authorized: webSearch,
      });
      setPatches(res.patches as PatchProposal[]);
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [
    notePath,
    selectedChapter,
    chapterGoal,
    noteContent,
    webSearch,
  ]);

  const runDocumentCheck = useCallback(async () => {
    if (!notePath) return;
    setLoading(true);
    setError(null);
    setPatches([]);
    setDocSummary(null);
    setDocIssues([]);
    try {
      const hash = await sha256Hex(noteContent);
      const res = await documentCheckExecute({
        target_path: notePath,
        content: noteContent,
        base_content_hash: hash,
        check_type: checkType,
        web_authorized: webSearch,
      });
      setDocSummary(res.analysis_summary ?? null);
      const issues: string[] = [];
      if (res.outline_result) {
        for (const i of res.outline_result.issues) {
          issues.push(`[大纲] ${i.description}`);
        }
      }
      if (res.citation_gap_result) {
        for (const c of res.citation_gap_result.uncited_claims) {
          issues.push(`[引用缺口] ${c.statement}`);
        }
      }
      if (res.style_result) {
        for (const i of res.style_result.inconsistencies) {
          issues.push(`[风格] ${i.description}`);
        }
      }
      setDocIssues(issues);
      setPatches((res.patches ?? []) as PatchProposal[]);
    } catch (e) {
      setError(invokeErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, [notePath, noteContent, checkType, webSearch]);

  const handleAcceptPatch = useCallback(
    async (patch: PatchProposal) => {
      try {
        const result = await patchApply(patch);
        if (!result.success) {
          setError(result.error ?? "补丁应用失败");
          return;
        }
        const before = noteContent.slice(0, patch.range.start);
        const after = noteContent.slice(patch.range.end);
        onPatchApplied?.(before + patch.replacement_text + after);
        setPatches((prev) => prev.filter((p) => p.id !== patch.id));
      } catch (e) {
        setError(invokeErrorMessage(e));
      }
    },
    [noteContent, onPatchApplied],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden p-3">
      <div className="flex shrink-0 gap-1">
        <button
          type="button"
          className={cn(
            "flex flex-1 items-center justify-center gap-1 rounded-md px-2 py-1 text-xs",
            mode === "chapter"
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground",
          )}
          onClick={() => setMode("chapter")}
        >
          <Layers className="h-3.5 w-3.5" />
          章节写作
        </button>
        <button
          type="button"
          className={cn(
            "flex flex-1 items-center justify-center gap-1 rounded-md px-2 py-1 text-xs",
            mode === "document"
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground",
          )}
          onClick={() => setMode("document")}
        >
          <ListChecks className="h-3.5 w-3.5" />
          文档检查
        </button>
      </div>

      {error ? (
        <p className="shrink-0 text-xs text-destructive">{error}</p>
      ) : null}

      {mode === "chapter" ? (
        <>
          <Card className="shrink-0 border-border/60">
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">选择章节</CardTitle>
            </CardHeader>
            <CardContent className="max-h-32 space-y-1 overflow-y-auto">
              {chapters.length === 0 ? (
                <p className="text-xs text-muted-foreground">无标题结构，将视全文为一章</p>
              ) : (
                chapters.map((ch) => (
                  <button
                    key={`${ch.content_start}-${ch.heading_text}`}
                    type="button"
                    className={cn(
                      "w-full rounded-md px-2 py-1 text-left text-xs",
                      selectedChapter?.content_start === ch.content_start
                        ? "bg-primary/10 text-foreground"
                        : "hover:bg-muted",
                    )}
                    onClick={() => setSelectedChapter(ch)}
                  >
                    <FileText className="mr-1 inline h-3 w-3" />
                    {ch.heading_path}
                  </button>
                ))
              )}
            </CardContent>
          </Card>
          <textarea
            className="min-h-[64px] shrink-0 resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm"
            placeholder="章节写作目标：改写、续写、重排结构…"
            value={chapterGoal}
            onChange={(e) => setChapterGoal(e.target.value)}
          />
          <Button
            type="button"
            size="sm"
            disabled={loading || !notePath || !selectedChapter}
            onClick={() => void runChapter()}
          >
            {loading ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : null}
            生成章节补丁
          </Button>
        </>
      ) : (
        <>
          <select
            className="shrink-0 rounded-md border border-border bg-background px-2 py-1.5 text-xs"
            value={checkType}
            onChange={(e) =>
              setCheckType(e.target.value as DocumentCheckType)
            }
          >
            {CHECK_TYPES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
          <Button
            type="button"
            size="sm"
            disabled={loading || !notePath}
            onClick={() => void runDocumentCheck()}
          >
            {loading ? (
              <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
            ) : null}
            运行文档检查
          </Button>
          {docSummary ? (
            <Card className="shrink-0 border-border/60">
              <CardHeader className="pb-1">
                <CardTitle className="text-xs font-medium">综合分析</CardTitle>
              </CardHeader>
              <CardContent className="whitespace-pre-wrap text-xs text-muted-foreground">
                {docSummary}
              </CardContent>
            </Card>
          ) : null}
          {docIssues.length > 0 ? (
            <ul className="shrink-0 max-h-24 list-disc overflow-y-auto pl-4 text-xs text-muted-foreground">
              {docIssues.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          ) : null}
        </>
      )}

      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto">
        {patches.map((patch) => (
          <PatchPreview
            key={patch.id}
            patch={patch}
            onAccept={(p) => void handleAcceptPatch(p)}
            onReject={(p) =>
              setPatches((prev) => prev.filter((x) => x.id !== p.id))
            }
            onCopy={(p) => void navigator.clipboard.writeText(p.replacement_text)}
            onRegenerate={() =>
              mode === "chapter" ? void runChapter() : void runDocumentCheck()
            }
          />
        ))}
      </div>
    </div>
  );
}
