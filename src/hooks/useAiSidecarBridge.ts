import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";

import {
  createEditorContextReference,
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
} from "@/lib/context-reference";
import {
  getWebSearchAvailability,
  type WebSearchProviderOption,
} from "@/lib/web-search-provider-state";
import {
  settingsGet,
  settingsSet,
  webEvidenceProvidersList,
  webSearchRouteGet,
  webSearchRouteSet,
} from "@/lib/ipc";
import {
  EMPTY_ASSISTANT_CHROME,
  type AssistantChromeSnapshot,
} from "@/types/assistant-chrome";
import type { ContextReference } from "@/types/ai";
import type { EditorSelectionCandidate } from "@/types/editor-selection";

const SELECTION_VALIDATION_DEBOUNCE_MS = 120;
const ALWAYS_CLEAN = () => false;

function selectionPreview(editor: Editor, from: number, to: number): string {
  return editor.state.doc
    .textBetween(from, to, " ")
    .replace(/\s+/gu, " ")
    .trim();
}

function selectionKey(
  editor: Editor,
  documentKey: string | null | undefined,
): string | null {
  const { from, to } = editor.state.selection;
  if (from === to) return null;
  return `${documentKey ?? ""}:${from}:${to}:${selectionPreview(editor, from, to)}`;
}

interface UseAiSidecarBridgeParams {
  editorRef: RefObject<Editor | null>;
  editor?: Editor | null;
  documentKey?: string | null;
  documentDirty?: boolean;
  assistantVisible?: boolean;
  selectionEnabled?: boolean;
  isDocumentDirty?: () => boolean;
  setAiStatus?: (message: string) => void;
}

export function useAiSidecarBridge({
  editorRef,
  editor = null,
  documentKey = null,
  documentDirty,
  assistantVisible = true,
  selectionEnabled = true,
  isDocumentDirty = ALWAYS_CLEAN,
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
  const [editorSelectionCandidate, setEditorSelectionCandidate] =
    useState<EditorSelectionCandidate | null>(null);
  const [assistantChrome, setAssistantChrome] =
    useState<AssistantChromeSnapshot>(EMPTY_ASSISTANT_CHROME);
  const selectionRequestGenerationRef = useRef(0);
  const selectionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const currentSelectionKeyRef = useRef<string | null>(null);
  const suppressedSelectionKeyRef = useRef<string | null>(null);
  const observedDocumentKeyRef = useRef(documentKey);
  const observedAssistantVisibleRef = useRef(assistantVisible);
  const isDocumentDirtyRef = useRef(isDocumentDirty);
  isDocumentDirtyRef.current = isDocumentDirty;
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      selectionRequestGenerationRef.current += 1;
      if (selectionTimerRef.current) clearTimeout(selectionTimerRef.current);
    };
  }, []);

  const webSearchAvailability = useMemo(
    () => getWebSearchAvailability(webSearchProviders, webSearchProviderId),
    [webSearchProviderId, webSearchProviders],
  );

  const refreshWebSearchProviders = useCallback(async () => {
    try {
      const [providers, route] = await Promise.all([
        webEvidenceProvidersList(),
        webSearchRouteGet(),
      ]);
      setWebSearchProviders(providers);
      setWebSearchProviderIdState(route.candidateProviderIds[0] ?? null);
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
      const [enabled, providers, route] = await Promise.all([
        settingsGet<boolean>("web_search_enabled").catch(() => false),
        webEvidenceProvidersList().catch(() => []),
        webSearchRouteGet().catch(() => ({ candidateProviderIds: [] })),
      ]);
      if (cancelled) return;
      const normalizedSelectedProviderId =
        route.candidateProviderIds[0] ?? null;
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
    void webSearchRouteSet({
      candidateProviderIds: normalized ? [normalized] : [],
    });
  }, []);

  const clearSelectionCandidate = useCallback(() => {
    selectionRequestGenerationRef.current += 1;
    if (selectionTimerRef.current) {
      clearTimeout(selectionTimerRef.current);
      selectionTimerRef.current = null;
    }
    setEditorSelectionCandidate(null);
  }, []);

  const validateSelection = useCallback(
    async (ed: Editor, key: string, preview: string) => {
      const generation = ++selectionRequestGenerationRef.current;
      if (documentDirty ?? isDocumentDirtyRef.current()) {
        setEditorSelectionCandidate({
          key,
          preview,
          status: "save_required",
          reference: null,
          message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
        });
        return;
      }
      const result = await createEditorContextReference({
        editor: ed,
        kind: "selection",
        isDirty: isDocumentDirtyRef.current,
      });
      if (
        !mountedRef.current ||
        generation !== selectionRequestGenerationRef.current ||
        currentSelectionKeyRef.current !== key
      ) {
        return;
      }
      if (!result.ok) {
        setEditorSelectionCandidate({
          key,
          preview,
          status: result.reason === "dirty" ? "save_required" : "invalid",
          reference: null,
          message: result.message,
        });
        return;
      }
      setEditorSelectionCandidate({
        key,
        preview,
        status: "ready",
        reference: result.reference,
        message: null,
      });
    },
    [documentDirty],
  );

  const syncSelectionCandidate = useCallback(() => {
    const ed = editor ?? editorRef.current;
    const documentChanged = observedDocumentKeyRef.current !== documentKey;
    const visibilityChanged =
      observedAssistantVisibleRef.current !== assistantVisible;
    if (documentChanged) {
      observedDocumentKeyRef.current = documentKey;
      currentSelectionKeyRef.current = null;
      suppressedSelectionKeyRef.current = null;
      clearSelectionCandidate();
      return;
    }
    if (visibilityChanged) {
      observedAssistantVisibleRef.current = assistantVisible;
      // A hidden Agent destroys the live candidate. Reopening it is an
      // explicit new presentation opportunity, so a manually dismissed or
      // already-consumed selection may be established again.
      suppressedSelectionKeyRef.current = null;
      currentSelectionKeyRef.current = null;
    }
    if (!ed || !assistantVisible || !selectionEnabled) {
      clearSelectionCandidate();
      return;
    }
    const key = selectionKey(ed, documentKey);
    currentSelectionKeyRef.current = key;
    if (!key) {
      suppressedSelectionKeyRef.current = null;
      clearSelectionCandidate();
      return;
    }
    if (suppressedSelectionKeyRef.current === key) {
      clearSelectionCandidate();
      return;
    }
    suppressedSelectionKeyRef.current = null;
    // Invalidate any in-flight disk verification before scheduling the newest
    // selection, including content updates that leave the same range intact.
    selectionRequestGenerationRef.current += 1;
    const { from, to } = ed.state.selection;
    const preview = selectionPreview(ed, from, to);
    setEditorSelectionCandidate({
      key,
      preview,
      status: "validating",
      reference: null,
      message: null,
    });
    if (selectionTimerRef.current) clearTimeout(selectionTimerRef.current);
    selectionTimerRef.current = setTimeout(() => {
      selectionTimerRef.current = null;
      void validateSelection(ed, key, preview);
    }, SELECTION_VALIDATION_DEBOUNCE_MS);
  }, [
    assistantVisible,
    clearSelectionCandidate,
    documentKey,
    editor,
    editorRef,
    selectionEnabled,
    validateSelection,
  ]);
  const syncSelectionCandidateRef = useRef(syncSelectionCandidate);
  syncSelectionCandidateRef.current = syncSelectionCandidate;

  useEffect(() => {
    const ed = editor ?? editorRef.current;
    if (!ed) {
      clearSelectionCandidate();
      return;
    }
    ed.on("selectionUpdate", syncSelectionCandidate);
    ed.on("update", syncSelectionCandidate);
    syncSelectionCandidate();
    return () => {
      ed.off("selectionUpdate", syncSelectionCandidate);
      ed.off("update", syncSelectionCandidate);
      clearSelectionCandidate();
    };
  }, [clearSelectionCandidate, editor, editorRef, syncSelectionCandidate]);

  useEffect(() => {
    syncSelectionCandidateRef.current();
  }, [assistantVisible, documentDirty, selectionEnabled]);

  const consumeEditorSelectionReference = useCallback(() => {
    suppressedSelectionKeyRef.current = currentSelectionKeyRef.current;
    clearSelectionCandidate();
  }, [clearSelectionCandidate]);

  const dismissEditorSelectionReference = consumeEditorSelectionReference;

  const sendSelectionToAi = useCallback(
    async (options?: { prefill?: string }) => {
      setPrefillMessage(options?.prefill ?? null);
      syncSelectionCandidate();
    },
    [syncSelectionCandidate],
  );

  const editorSelectionReference: ContextReference | null =
    editorSelectionCandidate?.reference ?? null;

  return {
    aiPanelOpen,
    assistantChrome,
    consumeEditorSelectionReference,
    dismissEditorSelectionReference,
    editorSelectionCandidate,
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
