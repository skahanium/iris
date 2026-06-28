import { useEffect, useRef, useState } from "react";

import { IrisMark } from "@/components/brand/IrisMark";
import { showMainWindowWhenReady } from "@/lib/ipc";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import { cn } from "@/lib/utils";

export interface StartupSplashProps {
  ready: boolean;
  minDurationMs?: number;
  fadeDurationMs?: number;
  onExited?: () => void;
}

type StartupSplashState = "visible" | "exiting" | "hidden";

let startupWindowRevealRequested = false;

function afterNextPaint(callback: () => void): () => void {
  let firstFrame = 0;
  let secondFrame = 0;
  firstFrame = window.requestAnimationFrame(() => {
    secondFrame = window.requestAnimationFrame(callback);
  });

  return () => {
    window.cancelAnimationFrame(firstFrame);
    window.cancelAnimationFrame(secondFrame);
  };
}

function prefersReducedMotion(): boolean {
  return (
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false
  );
}

export function StartupSplash({
  ready,
  minDurationMs = 1600,
  fadeDurationMs = 220,
  onExited,
}: StartupSplashProps) {
  const mountedAtRef = useRef(Date.now());
  const exitedRef = useRef(false);
  const [state, setState] = useState<StartupSplashState>("visible");
  const [reducedMotion, setReducedMotion] = useState(prefersReducedMotion);

  useEffect(() => {
    if (!isTauriRuntime() || startupWindowRevealRequested) return;
    return afterNextPaint(() => {
      if (startupWindowRevealRequested) return;
      startupWindowRevealRequested = true;
      void showMainWindowWhenReady().catch((error: unknown) => {
        console.error("Failed to show Iris startup window", error);
      });
    });
  }, []);

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (!query) return;
    const handleChange = () => setReducedMotion(query.matches);
    query.addEventListener?.("change", handleChange);
    return () => query.removeEventListener?.("change", handleChange);
  }, []);

  useEffect(() => {
    if (!ready || state !== "visible") return;
    const elapsed = Date.now() - mountedAtRef.current;
    const remaining = Math.max(0, minDurationMs - elapsed);
    if (remaining === 0) {
      setState("exiting");
      return;
    }
    const timer = window.setTimeout(() => setState("exiting"), remaining);
    return () => window.clearTimeout(timer);
  }, [minDurationMs, ready, state]);

  useEffect(() => {
    if (state !== "exiting") return;
    const timer = window.setTimeout(() => {
      exitedRef.current = true;
      setState("hidden");
      onExited?.();
    }, fadeDurationMs);
    return () => window.clearTimeout(timer);
  }, [fadeDurationMs, onExited, state]);

  useEffect(() => {
    return () => {
      if (!exitedRef.current && state === "hidden") {
        onExited?.();
      }
    };
  }, [onExited, state]);

  if (state === "hidden") return null;

  return (
    <div
      data-testid="startup-splash"
      data-state={state}
      className={cn(
        "iris-startup-splash",
        state === "exiting" && "iris-startup-splash--exiting",
        reducedMotion && "iris-startup-splash--reduced-motion",
      )}
      role="status"
      aria-live="polite"
      aria-label="Iris 正在启动"
    >
      <div className="iris-startup-orbit-stage" aria-hidden="true">
        <span className="iris-startup-orbit iris-startup-orbit--outer" />
        <span className="iris-startup-orbit iris-startup-orbit--middle" />
        <span className="iris-startup-orbit iris-startup-orbit--inner" />
        <span className="iris-startup-node iris-startup-node--a" />
        <span className="iris-startup-node iris-startup-node--b" />
        <span className="iris-startup-node iris-startup-node--c" />
        <span className="iris-startup-pulse" />
        <div className="iris-startup-mark-shell">
          <IrisMark size={42} title="Iris" />
        </div>
      </div>
      <div className="iris-startup-copy">
        <p className="iris-startup-title">唤醒知识网络</p>
        <p className="iris-startup-status">
          {state === "exiting" ? "打开工作区" : "准备笔记"}
        </p>
      </div>
    </div>
  );
}
