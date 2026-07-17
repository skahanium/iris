import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type RefObject,
} from "react";

import { createEditorContextReference } from "@/lib/context-reference";
import {
  getWebSearchAvailability,
  type WebSearchProviderOption,
} from "@/lib/web-search-provider-state";
import { settingsGet, settingsSet, webEvidenceProvidersList } from "@/lib/ipc";
import {
  EMPTY_ASSISTANT_CHROME,
  type AssistantChromeSnapshot,
} from "@/types/assistant-chrome";
import type { ContextReference } from "@/types/ai";

interface UseAiSidecarBridgeParams {
  editorRef: RefObject<Editor | null>;
  isDocumentDirty?: () => boolean;
  setAiStatus: (message: string) => void;
}

export function useAiSidecarBridge({
  editorRef,
  isDocumentDirty = () => false,
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
  const [prefillMessage, setPrefillMessage] = useState<string | null>(null);
  const [editorSelectionReference, setEditorSelectionReference] =
    useState<ContextReference | null>(null);
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
    async (options?: { prefill?: string }) => {
      const ed = editorRef.current;
      if (!ed) {
        setEditorSelectionReference(null);
        setAiStatus("请先在编辑器中选中文本");
        return;
      }
      const result = await createEditorContextReference({
        editor: ed,
        kind: "selection",
        isDirty: isDocumentDirty,
      });
      if (!result.ok) {
        setEditorSelectionReference(null);
        setAiStatus(result.message);
        return;
      }
      setEditorSelectionReference(result.reference);
      setPrefillMessage(options?.prefill ?? null);
      setAiPanelOpen(true);
    },
    [editorRef, isDocumentDirty, setAiStatus],
  );

  return {
    aiPanelOpen,
    assistantChrome,
    editorSelectionReference,
    prefillMessage,
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
