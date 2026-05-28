import { useCallback, useEffect, useRef, useState } from "react";

interface InlineSuggestion {
  text: string;
  confidence: number;
  source: string;
}

const DEBOUNCE_MS = 500;

export function useInlineSuggestion() {
  const [suggestion, setSuggestion] = useState<InlineSuggestion | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const fetchSuggestion = useCallback(
    (context: string, cursorPosition: number) => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }

      timerRef.current = setTimeout(async () => {
        if (abortControllerRef.current) {
          abortControllerRef.current.abort();
        }

        abortControllerRef.current = new AbortController();
        setIsLoading(true);

        try {
          void context;
          void cursorPosition;
          // TODO: 调用后端 API 获取建议
          // const result = await ipc.getSuggestion({ context, cursorPosition });
          // setSuggestion(result);
        } catch (error) {
          if (error instanceof Error && error.name !== "AbortError") {
            console.error("Failed to fetch suggestion:", error);
          }
        } finally {
          setIsLoading(false);
        }
      }, DEBOUNCE_MS);
    },
    [],
  );

  const acceptSuggestion = useCallback(() => {
    if (suggestion) {
      // TODO: 插入建议文本到编辑器
      setSuggestion(null);
    }
  }, [suggestion]);

  const dismissSuggestion = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    setSuggestion(null);
  }, []);

  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  return {
    suggestion,
    isLoading,
    fetchSuggestion,
    acceptSuggestion,
    dismissSuggestion,
  } as const;
}
