import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import { sanitizeAssistantVisibleText } from "@/lib/assistant-visible-text";
import {
  fallbackLineBudget,
  fontSizePxFromFont,
  measureTextForBudget,
  nextRevealLength,
  stableGraphemePrefix,
  type StreamingLineBudget,
} from "@/lib/streaming-line-fit";

export type { StreamingLineBudget };

/** Local-only playback metadata. It never changes the durable Run contract. */
export interface AssistantAnswerPresentation {
  runId: string;
  resetEpoch: number;
  complete: boolean;
  stopped?: boolean;
  settled?: boolean;
  initialVisibleLength?: number;
}

type RevealInput = Pick<
  AssistantPresentationState,
  "runId" | "answer" | "answerComplete" | "resetEpoch"
> & { stopped?: boolean; settled?: boolean; initialVisibleLength?: number };

export interface AssistantAnswerReveal {
  runId: string | null;
  answer: string;
  revealing: boolean;
}

function reducedMotion(): boolean {
  return (
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false
  );
}

/** Time-paced, run-isolated playback owned by the current answer component. */
export function useAssistantAnswerReveal(
  presentation: RevealInput | null,
  getLineBudget?: () => StreamingLineBudget | null,
): AssistantAnswerReveal & {
  phase: "receiving" | "draining" | "complete" | "stopped";
  revealAll: () => void;
} {
  const runId = presentation?.runId ?? null;
  const resetEpoch = presentation?.resetEpoch ?? 0;
  const stopped = presentation?.stopped ?? false;
  const immediate = presentation?.settled || reducedMotion();
  const key = `${runId ?? ""}:${resetEpoch}`;
  const target = useMemo(() => {
    const safe = sanitizeAssistantVisibleText(presentation?.answer ?? "");
    return presentation?.answerComplete || immediate
      ? safe
      : stableGraphemePrefix(safe);
  }, [presentation?.answer, presentation?.answerComplete, immediate]);
  const initialAnswer = target.slice(
    0,
    presentation?.initialVisibleLength ?? 0,
  );
  const [answer, setAnswer] = useState(initialAnswer);
  const [visibilityEpoch, setVisibilityEpoch] = useState(0);
  const answerRef = useRef(initialAnswer);
  const targetRef = useRef("");
  const keyRef = useRef(key);
  const frameRef = useRef<number | null>(null);
  const previousTimeRef = useRef<number | null>(null);
  const creditRef = useRef(0);
  const rateRef = useRef(4);
  const getBudgetRef = useRef(getLineBudget);
  getBudgetRef.current = getLineBudget;

  const cancelFrame = useCallback(() => {
    if (frameRef.current !== null)
      window.cancelAnimationFrame(frameRef.current);
    frameRef.current = null;
    previousTimeRef.current = null;
  }, []);

  const revealAll = useCallback(() => {
    cancelFrame();
    creditRef.current = 0;
    answerRef.current = targetRef.current;
    setAnswer(targetRef.current);
  }, [cancelFrame]);

  useEffect(() => {
    const reset =
      keyRef.current !== key || !target.startsWith(answerRef.current);
    if (reset) {
      cancelFrame();
      keyRef.current = key;
      answerRef.current = "";
      creditRef.current = 0;
      rateRef.current = 4;
      setAnswer("");
    }
    targetRef.current = target;
    if (runId === null || stopped) {
      cancelFrame();
      return;
    }
    if (immediate) {
      revealAll();
      return;
    }
    if (answerRef.current === target || frameRef.current !== null) return;

    const tick = (now: number) => {
      frameRef.current = null;
      if (keyRef.current !== key) return;
      if (document.visibilityState === "hidden") {
        previousTimeRef.current = null;
        return;
      }
      const dt =
        previousTimeRef.current === null
          ? 1000 / 60
          : Math.max(0, Math.min(50, now - previousTimeRef.current));
      previousTimeRef.current = now;
      const budget = getBudgetRef.current?.() ?? fallbackLineBudget();
      const width = Math.max(1, budget.lineWidthPx);
      const latest = targetRef.current;
      const current = answerRef.current;
      const pendingChars = latest.length - current.length;
      const pendingLines =
        (pendingChars * fontSizePxFromFont(budget.font)) / width;
      const desiredRate = 4 + 16 * (1 - Math.exp(-pendingLines / 8));
      rateRef.current +=
        (desiredRate - rateRef.current) * (1 - Math.exp(-dt / 250));
      creditRef.current = Math.min(
        width,
        creditRef.current + (width * rateRef.current * dt) / 1000,
      );
      const end = nextRevealLength({
        current,
        target: latest,
        remainingPx: budget.remainingPx,
        lineWidthPx: width,
        maxAdvancePx: creditRef.current,
        measure: (text) => measureTextForBudget(text, budget.font),
      });
      if (end > current.length) {
        const delta = latest.slice(current.length, end);
        const cost = delta.includes("\n")
          ? width
          : measureTextForBudget(delta, budget.font);
        creditRef.current = Math.max(0, creditRef.current - cost);
        answerRef.current = latest.slice(0, end);
        setAnswer(answerRef.current);
      }
      if (answerRef.current.length < latest.length) {
        frameRef.current = window.requestAnimationFrame(tick);
      } else {
        previousTimeRef.current = null;
        creditRef.current = 0;
      }
    };
    frameRef.current = window.requestAnimationFrame(tick);
  }, [
    cancelFrame,
    immediate,
    key,
    revealAll,
    runId,
    stopped,
    target,
    visibilityEpoch,
  ]);

  // A hidden window resumes with a fresh clock, without accumulating time debt.
  useEffect(() => {
    const onVisibility = () => {
      if (document.visibilityState === "visible")
        setVisibilityEpoch((value) => value + 1);
      else cancelFrame();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelFrame();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [cancelFrame]);

  const valid = keyRef.current === key && target.startsWith(answer);
  const visibleAnswer = immediate && !stopped ? target : valid ? answer : "";
  const revealing =
    runId !== null && !stopped && visibleAnswer.length < target.length;
  return {
    runId,
    answer: visibleAnswer,
    revealing,
    phase: stopped
      ? "stopped"
      : !presentation?.answerComplete
        ? "receiving"
        : revealing
          ? "draining"
          : "complete",
    revealAll,
  };
}
