import { CircleAlert, LoaderCircle, TextQuote, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { contextReferenceDisplayText } from "@/lib/context-reference";
import { cn } from "@/lib/utils";
import type { ContextReference } from "@/types/ai";
import type { EditorSelectionCandidate } from "@/types/editor-selection";

interface AssistantContextShelfProps {
  candidate: EditorSelectionCandidate | null;
  contextReferences: ContextReference[];
  composerDisabled: boolean;
  streaming: boolean;
  onDismissCandidate?: () => void;
  onRemoveReference: (id: string) => void;
}

function CandidateStatus({
  status,
}: {
  status: EditorSelectionCandidate["status"];
}) {
  if (status === "ready") return null;
  if (status === "validating") {
    return (
      <span className="iris-context-shelf-status">
        <LoaderCircle aria-hidden="true" className="h-3 w-3 animate-spin" />
        校验中
      </span>
    );
  }
  return (
    <span className="iris-context-shelf-status">
      <CircleAlert aria-hidden="true" className="h-3 w-3" />
      {status === "save_required" ? "保存后可引用" : "无法引用"}
    </span>
  );
}

export function AssistantContextShelf({
  candidate,
  contextReferences,
  composerDisabled,
  streaming,
  onDismissCandidate,
  onRemoveReference,
}: AssistantContextShelfProps) {
  const [visibleCandidate, setVisibleCandidate] =
    useState<EditorSelectionCandidate | null>(candidate);
  const [candidateExiting, setCandidateExiting] = useState(false);
  const exitTimerRef = useRef<number | null>(null);
  const visibleCandidateRef = useRef(visibleCandidate);
  visibleCandidateRef.current = visibleCandidate;

  useEffect(() => {
    if (exitTimerRef.current !== null) {
      window.clearTimeout(exitTimerRef.current);
      exitTimerRef.current = null;
    }
    if (candidate) {
      setVisibleCandidate(candidate);
      setCandidateExiting(false);
      return;
    }
    if (!visibleCandidateRef.current) return;
    const reducedMotion =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reducedMotion) {
      setVisibleCandidate(null);
      setCandidateExiting(false);
      return;
    }
    setCandidateExiting(true);
    exitTimerRef.current = window.setTimeout(() => {
      setVisibleCandidate(null);
      setCandidateExiting(false);
      exitTimerRef.current = null;
    }, 160);
    return () => {
      if (exitTimerRef.current !== null) {
        window.clearTimeout(exitTimerRef.current);
        exitTimerRef.current = null;
      }
    };
  }, [candidate]);

  useEffect(
    () => () => {
      if (exitTimerRef.current !== null)
        window.clearTimeout(exitTimerRef.current);
    },
    [],
  );

  if (!visibleCandidate && contextReferences.length === 0) return null;

  return (
    <div className="iris-context-shelf" data-testid="context-shelf">
      {visibleCandidate ? (
        <div
          className={cn(
            "iris-context-shelf-candidate",
            candidateExiting
              ? "iris-context-shelf-exit"
              : "iris-context-shelf-enter",
          )}
          data-testid="editor-selection-candidate"
          aria-hidden={candidateExiting || undefined}
        >
          <span data-context-leading-marker aria-hidden="true" />
          <TextQuote
            aria-hidden="true"
            className="h-4 w-4 shrink-0 text-primary/70"
          />
          <span className="min-w-0 flex-1 truncate">
            <span className="mr-1.5 font-medium text-foreground">当前选区</span>
            <span
              className="text-muted-foreground"
              title={visibleCandidate.preview}
            >
              {visibleCandidate.preview || "未命名选区"}
            </span>
          </span>
          <CandidateStatus status={visibleCandidate.status} />
          <button
            type="button"
            aria-label="移除当前选区引用"
            className="iris-context-shelf-remove"
            disabled={composerDisabled || streaming}
            onClick={onDismissCandidate}
          >
            <X aria-hidden="true" className="h-3.5 w-3.5" />
          </button>
        </div>
      ) : null}
      {contextReferences.length > 0 ? (
        <div
          className="iris-context-shelf-references"
          data-testid="context-reference-boundary"
        >
          <span className="iris-context-shelf-heading">已收集引用</span>
          <span className="iris-context-shelf-hint">随本条问题一并发送</span>
          <div className="flex min-w-0 flex-wrap gap-1.5">
            {contextReferences.map((reference) => (
              <span
                key={reference.id}
                className="iris-context-reference-chip"
                title={contextReferenceDisplayText(reference)}
              >
                <span className="truncate">
                  {contextReferenceDisplayText(reference)}
                </span>
                <button
                  type="button"
                  aria-label={`移除引用 ${reference.id}`}
                  className="iris-context-shelf-remove"
                  disabled={composerDisabled || streaming}
                  onClick={() => onRemoveReference(reference.id)}
                >
                  <X aria-hidden="true" className="h-3 w-3" />
                </button>
              </span>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
