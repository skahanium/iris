import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type RefObject,
} from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import { getEditorSelectionSnapshot } from "@/lib/iris-clipboard";
import {
  getWebSearchAvailability,
  type WebSearchProviderOption,
} from "@/lib/web-search-provider-state";
import { settingsGet, settingsSet, webEvidenceProvidersList } from "@/lib/ipc";
import {
  EMPTY_ASSISTANT_CHROME,
  type AssistantChromeSnapshot,
} from "@/types/assistant-chrome";

interface UseAiSidecarBridgeParams {
  activePathRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  getNoteContent: () => string;
  setAiStatus: (message: string) => void;
}

export function useAiSidecarBridge({
  activePathRef,
  editorRef,
  getNoteContent,
  setAiStatus,
}: UseAiSidecarBridgeParams) {
  const [aiPanelOpen, setAiPanelOpen] = useState(true);
  const [webSearchEnabled, setWebSearchEnabled] = useState(false);
  const [webSearchProviders, setWebSearchProviders] = useState<
    WebSearchProviderOption[]
  >([]);
  const [webSearchProviderId, setWebSearchProviderIdState] = useState<
    string | null
  >(null);
  const [webSearchProvidersLoaded, setWebSearchProvidersLoaded] =
    useState(false);
  const [selectionQuote, setSelectionQuote] =
    useState<AssistantSelectionQuote | null>(null);
  const [prefillMessage, setPrefillMessage] = useState<string | null>(null);
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);

  const webSearchAvailability = useMemo(
    () => getWebSearchAvailability(webSearchProviders, webSearchProviderId),
    [webSearchProviderId, webSearchProviders],
  );

  const refreshWebSearchProviders = useCallback(async () => {
    try {
      const [providers, selectedProviderId] = await Promise.all([
        webEvidenceProvidersList(),
        settingsGet<string | null>("web_search_provider_id"),
      ]);
      setWebSearchProviders(providers);
      setWebSearchProviderIdState(
        typeof selectedProviderId === "string" ? selectedProviderId : null,
      );
    } catch {
      setWebSearchProviders([]);
      setWebSearchProviderIdState(null);
    } finally {
      setWebSearchProvidersLoaded(true);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [enabled, providers, selectedProviderId] = await Promise.all([
        settingsGet<boolean>("web_search_enabled").catch(() => false),
        webEvidenceProvidersList().catch(() => []),
        settingsGet<string | null>("web_search_provider_id").catch(() => null),
      ]);
      if (cancelled) return;
      const normalizedSelectedProviderId =
        typeof selectedProviderId === "string" ? selectedProviderId : null;
      const availability = getWebSearchAvailability(
        providers,
        normalizedSelectedProviderId,
      );
      setWebSearchProviders(providers);
      setWebSearchProviderIdState(normalizedSelectedProviderId);
      setWebSearchProvidersLoaded(true);
      const nextEnabled = enabled === true && availability.canEnable;
      setWebSearchEnabled(nextEnabled);
      if (enabled === true && !availability.canEnable) {
        void settingsSet("web_search_enabled", false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (
      webSearchProvidersLoaded &&
      webSearchEnabled &&
      !webSearchAvailability.canEnable
    ) {
      setWebSearchEnabled(false);
      void settingsSet("web_search_enabled", false);
    }
  }, [
    webSearchAvailability.canEnable,
    webSearchEnabled,
    webSearchProvidersLoaded,
  ]);

  const setWebSearch = useCallback(
    (enabled: boolean) => {
      const nextEnabled = enabled && webSearchAvailability.canEnable;
      setWebSearchEnabled(nextEnabled);
      void settingsSet("web_search_enabled", nextEnabled);
    },
    [webSearchAvailability.canEnable],
  );

  const toggleWebSearch = useCallback(() => {
    setWebSearchEnabled((prev) => {
      const next = !prev && webSearchAvailability.canEnable;
      void settingsSet("web_search_enabled", next);
      return next;
    });
  }, [webSearchAvailability.canEnable]);

  const setWebSearchProviderId = useCallback((providerId: string | null) => {
    const normalized = providerId?.trim() || null;
    setWebSearchProviderIdState(normalized);
    void settingsSet("web_search_provider_id", normalized);
  }, []);

  const sendSelectionToAi = useCallback(
    (options?: { prefill?: string }) => {
      const ed = editorRef.current;
      const path = activePathRef.current;
      if (!ed || !path) return;
      const snapshot = getEditorSelectionSnapshot(ed);
      if (!snapshot) {
        setAiStatus("请先在编辑器中选中文本");
        return;
      }
      const classifiedSelection = isClassifiedVaultPath(path);
      setSelectionQuote({
        filePath: path,
        text: snapshot.text,
        content: classifiedSelection ? snapshot.text : getNoteContent(),
        editorRange: snapshot.editorRange,
      });
      setPrefillMessage(options?.prefill ?? null);
      setAiPanelOpen(true);
    },
    [activePathRef, editorRef, getNoteContent, setAiStatus],
  );

  return {
    aiPanelOpen,
    assistantChrome,
    prefillMessage,
    selectionQuote,
    setAiPanelOpen,
    setAssistantChrome,
    setWebSearch,
    setWebSearchProviderId,
    sendSelectionToAi,
    toggleWebSearch,
    refreshWebSearchProviders,
    webSearchAvailability,
    webSearchEnabled,
    webSearchProviderId,
    webSearchProviders,
  };
}
