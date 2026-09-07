import { useEffect, useMemo, useRef, useState } from "react";

import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";
import {
  fallbackLineBudget,
  measureTextForBudget,
  nextRevealLength,
  type StreamingLineBudget,
} from "@/lib/streaming-line-fit";

export type { StreamingLineBudget };

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function revealNext(
  current: string,
  target: string,
  budget: StreamingLineBudget,
): string {
  const end = nextRevealLength({
    current,
    target,
    remainingPx: budget.remainingPx,
    lineWidthPx: budget.lineWidthPx,
    measure: (text) => measureTextForBudget(text, budget.font),
  });
  return target.slice(0, Math.min(target.length, end));
}

export interface AssistantAnswerReveal {
  /** The Run whose presentation answer this reveal currently represents. */
  runId: string | null;
  /** The smoothed text that should be rendered right now. */
  answer: string;
  /** True while the live answer still has buffered text to reveal. */
  revealing: boolean;
}

/**
 * Returns a line-paced slice of the live presentation answer.
 *
 * The authoritative `presentation.answer` is never mutated. Visible text
 * fills the current visual line and only then wraps; a new empty line is
 * capped to a fraction of the measured line width so the last line grows
 * instead of appearing all at once.
 */
export function useAssistantAnswerReveal(
  presentation: AssistantPresentationState | null,
  getLineBudget?: () => StreamingLineBudget | null,
): AssistantAnswerReveal {
  const runId = presentation?.runId ?? null;
  const resetEpoch = presentation?.resetEpoch ?? 0;
  const target = useMemo(
    () => sanitizeAssistantVisibleText(presentation?.answer ?? ""),
    [presentation?.answer],
  );

  const [answer, setAnswer] = useState("");
  const answerRef = useRef("");
  const targetRef = useRef("");
  const frameRef = useRef<number | null>(null);
  const runIdRef = useRef<string | null>(null);
  const resetEpochRef = useRef<number | null>(null);
  const getLineBudgetRef = useRef(getLineBudget);
  getLineBudgetRef.current = getLineBudget;

  targetRef.current = target;

  const currentBudget = (): StreamingLineBudget =>
    getLineBudgetRef.current?.() ?? fallbackLineBudget();

  useEffect(() => {
    if (runIdRef.current !== runId || resetEpochRef.current !== resetEpoch) {
      runIdRef.current = runId;
      resetEpochRef.current = resetEpoch;
      answerRef.current = "";
      setAnswer("");
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    }

    if (
      target.length <= answerRef.current.length &&
      target !== answerRef.current
    ) {
      answerRef.current = target;
      setAnswer(target);
    }
  }, [resetEpoch, runId, target]);

  useEffect(() => {
    if (runId === null) return;
    if (target.length <= answerRef.current.length) return;

    if (prefersReducedMotion()) {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      if (answerRef.current !== target) {
        answerRef.current = target;
        setAnswer(target);
      }
      return;
    }

    const next = revealNext(answerRef.current, target, currentBudget());
    if (next === target) {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
      if (answerRef.current !== target) {
        answerRef.current = target;
        setAnswer(target);
      }
      return;
    }

    const tick = () => {
      frameRef.current = null;
      const latestTarget = targetRef.current;
      const current = answerRef.current;
      if (latestTarget.length <= current.length) {
        if (current !== latestTarget) {
          answerRef.current = latestTarget;
          setAnswer(latestTarget);
        }
        return;
      }

      const revealed = revealNext(current, latestTarget, currentBudget());
      answerRef.current = revealed;
      setAnswer(revealed);

      if (revealed.length < latestTarget.length) {
        frameRef.current = window.requestAnimationFrame(tick);
      }
    };

    if (frameRef.current === null) {
      frameRef.current = window.requestAnimationFrame(tick);
    }
  }, [runId, target]);

  useEffect(() => {
    return () => {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }
    };
  }, []);

  const answerBelongsToRun = runIdRef.current === runId;
  const visibleAnswer = answerBelongsToRun ? answer : "";

  return {
    runId,
    answer: visibleAnswer,
    revealing:
      runId !== null &&
      (!answerBelongsToRun || visibleAnswer.length < target.length),
  };
}
