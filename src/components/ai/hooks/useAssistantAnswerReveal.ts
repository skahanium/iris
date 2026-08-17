import { useEffect, useMemo, useRef, useState } from "react";

import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";

/**
 * Streaming answer reveal budget.
 *
 * The backend may deliver an `answer_delta` containing a whole paragraph at
 * once, especially through proxies or non-token-aligned providers. Applying
 * such a delta to the conversation in one React commit makes the viewport jump
 * and flicker. This hook releases the authoritative presentation answer in
 * small per-frame increments so the UI stays smooth even when the source chunk
 * is large.
 */
export const ASSISTANT_ANSWER_REVEAL_MIN_STEP = 2;
export const ASSISTANT_ANSWER_REVEAL_MAX_STEP = 48;
export const ASSISTANT_ANSWER_REVEAL_DRAIN_FRAMES = 24;

export function assistantAnswerRevealStep(pending: number): number {
  if (pending <= 0) return 0;
  return Math.min(
    ASSISTANT_ANSWER_REVEAL_MAX_STEP,
    Math.max(
      ASSISTANT_ANSWER_REVEAL_MIN_STEP,
      Math.ceil(pending / ASSISTANT_ANSWER_REVEAL_DRAIN_FRAMES),
    ),
  );
}

/** Avoid splitting a UTF-16 surrogate pair when slicing the reveal window. */
function alignEndToCodePoint(value: string, end: number): number {
  if (end <= 0 || end >= value.length) return end;
  const code = value.charCodeAt(end - 1);
  if (code >= 0xd800 && code <= 0xdbff) return end + 1;
  return end;
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export interface AssistantAnswerReveal {
  /** The smoothed text that should be rendered right now. */
  answer: string;
  /** True while the live answer still has buffered text to reveal. */
  revealing: boolean;
}

/**
 * Returns a smoothly revealed slice of the live presentation answer.
 *
 * The authoritative `presentation.answer` is never mutated; this hook only
 * controls how much of it is visible in the conversation. New runs and resets
 * clear immediately; large backlogs drain over a small number of animation
 * frames instead of one large DOM write.
 */
export function useAssistantAnswerReveal(
  presentation: AssistantPresentationState | null,
): AssistantAnswerReveal {
  const runId = presentation?.runId ?? null;
  const target = useMemo(
    () => sanitizeAssistantVisibleText(presentation?.answer ?? ""),
    [presentation?.answer],
  );

  const [answer, setAnswer] = useState("");
  const answerRef = useRef("");
  const targetRef = useRef("");
  const frameRef = useRef<number | null>(null);
  const runIdRef = useRef<string | null>(null);

  targetRef.current = target;

  useEffect(() => {
    if (runIdRef.current !== runId) {
      runIdRef.current = runId;
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
  }, [runId, target]);

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

    const tick = () => {
      frameRef.current = null;
      const latestTarget = targetRef.current;
      const current = answerRef.current;
      const pending = latestTarget.length - current.length;

      if (pending <= 0) {
        if (current !== latestTarget) {
          answerRef.current = latestTarget;
          setAnswer(latestTarget);
        }
        return;
      }

      const step = assistantAnswerRevealStep(pending);
      const nextLength = alignEndToCodePoint(
        latestTarget,
        Math.min(latestTarget.length, current.length + step),
      );
      const next = latestTarget.slice(0, nextLength);
      answerRef.current = next;
      setAnswer(next);

      if (nextLength < latestTarget.length) {
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

  return {
    answer,
    revealing: answer.length < target.length,
  };
}
