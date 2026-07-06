import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { listenLlmDone, listenLlmError, listenLlmToken } from "@/lib/ipc";
import { getAiPayloadStore, projectTextForUi } from "@/lib/ai-payload-store";
import { AssistantStreamBuffer } from "@/lib/assistant-stream-buffer";
import type { LlmTokenEvent } from "@/types/ipc";

/**
 * Streams the document-check `analysis_summary` into the doc panel token by
 * token. Gated on a dedicated `docStreamActiveRef` (not the chat panel ref) so
 * document tokens route to `setDocSummary` instead of the chat message list.
 */
export function useDocSummaryStream(options: {
  docStreamActiveRef: MutableRefObject<boolean>;
  requestIdRef: MutableRefObject<string | null>;
  setDocSummary: Dispatch<SetStateAction<string | null>>;
}) {
  const { docStreamActiveRef, requestIdRef, setDocSummary } = options;

  const bufRef = useRef(new AssistantStreamBuffer());
  const payloadStoreRef = useRef(getAiPayloadStore());
  const rafRef = useRef<number | undefined>(undefined);
  const lastFlushRef = useRef<number>(0);

  useEffect(() => {
    let disposed = false;
    let unlistenToken: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    function flushSnapshot() {
      const snapshot = bufRef.current.toString();
      const projection = projectTextForUi(payloadStoreRef.current, snapshot, {
        kind: "document_summary",
        maxPreviewChars: 36_000,
      });
      setDocSummary(projection.content);
    }

    void listenLlmToken((ev: LlmTokenEvent) => {
      if (disposed || !docStreamActiveRef.current) return;
      if (!requestIdRef.current) {
        requestIdRef.current = ev.request_id;
      } else if (ev.request_id !== requestIdRef.current) {
        return;
      }
      bufRef.current.append(ev.token);

      if (rafRef.current === undefined) {
        const elapsed = performance.now() - lastFlushRef.current;
        const delay = elapsed < 50 ? 50 - elapsed : 0;
        rafRef.current = window.setTimeout(() => {
          rafRef.current = undefined;
          if (disposed) return;
          lastFlushRef.current = performance.now();
          flushSnapshot();
        }, delay) as unknown as number;
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlistenToken = fn;
    });

    void listenLlmDone((ev) => {
      if (disposed || !docStreamActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
        flushSnapshot();
      }
      // The authoritative analysis_summary arrives via the IPC result; the
      // task runner's finally clears the active ref. Do not end early here.
    }).then((fn) => {
      if (disposed) fn();
      else unlistenDone = fn;
    });

    void listenLlmError((ev) => {
      if (disposed || !docStreamActiveRef.current) return;
      if (
        requestIdRef.current &&
        ev.request_id &&
        ev.request_id !== requestIdRef.current
      ) {
        return;
      }
      docStreamActiveRef.current = false;
      bufRef.current.clear();
      requestIdRef.current = null;
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlistenError = fn;
    });

    return () => {
      disposed = true;
      if (rafRef.current !== undefined) {
        clearTimeout(rafRef.current);
        rafRef.current = undefined;
      }
      unlistenToken?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, [docStreamActiveRef, requestIdRef, setDocSummary]);
}
