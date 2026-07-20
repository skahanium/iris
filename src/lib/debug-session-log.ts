/** Session-scoped debug ingest for agent debugging (safe no-op if unreachable). */
export function debugSessionLog(payload: {
  hypothesisId: string;
  location: string;
  message: string;
  data?: Record<string, unknown>;
  runId?: string;
}): void {
  const body = JSON.stringify({
    sessionId: "6556f7",
    runId: payload.runId ?? "pre-fix",
    hypothesisId: payload.hypothesisId,
    location: payload.location,
    message: payload.message,
    data: payload.data ?? {},
    timestamp: Date.now(),
  });
  // Prefer same-origin Vite middleware (CSP 'self'); keep localhost ingest as fallback.
  void fetch("/__iris_debug_ingest", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  }).catch(() => {
    void fetch(
      "http://127.0.0.1:7413/ingest/3336dc9b-75d7-44cd-8238-25a3e4a38bb9",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "6556f7",
        },
        body,
      },
    ).catch(() => {});
  });
}
