import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent,
  type RefObject,
  type SetStateAction,
} from "react";

import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import {
  buildMentionCandidates,
  findActiveMentionQuery,
  insertMentionToken,
  parseMentionTokens,
  tokensToContextScope,
  type MentionCandidate,
  type MentionToken,
} from "@/lib/ai-context-scope";
import { fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface UseAssistantContextScopeOptions {
  input: string;
  setInput: Dispatch<SetStateAction<string>>;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  loadVaultFiles?: () => Promise<FileListItem[]>;
  runtimeDocumentCandidates?: FileListItem[];
}

export function useAssistantContextScope({
  input,
  setInput,
  textareaRef,
  loadVaultFiles = fileList,
  runtimeDocumentCandidates = [],
}: UseAssistantContextScopeOptions) {
  const [vaultFiles, setVaultFiles] = useState<FileListItem[]>([]);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionStart, setMentionStart] = useState(0);
  const [mentionQuery, setMentionQuery] = useState("");
  const [mentionPrefix, setMentionPrefix] = useState<"@" | "#">("@");
  const loadSeqRef = useRef(0);

  const mentionTokens = useMemo(() => parseMentionTokens(input), [input]);
  const contextScope = useMemo(
    () => tokensToContextScope(mentionTokens),
    [mentionTokens],
  );
  const mentionSourceFiles = useMemo(() => {
    const byPath = new Map<string, FileListItem>();
    for (const item of vaultFiles) byPath.set(item.path, item);
    for (const item of runtimeDocumentCandidates) byPath.set(item.path, item);
    return [...byPath.values()];
  }, [runtimeDocumentCandidates, vaultFiles]);
  const mentionCandidates = useMemo(
    () =>
      mentionOpen
        ? buildMentionCandidates(mentionSourceFiles, mentionQuery)
        : [],
    [mentionOpen, mentionSourceFiles, mentionQuery],
  );

  const refreshVaultFiles = useCallback(() => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    return loadVaultFiles()
      .then((files) => {
        if (loadSeqRef.current === seq) setVaultFiles(files);
      })
      .catch(() => {
        if (loadSeqRef.current === seq) setVaultFiles([]);
      });
  }, [loadVaultFiles]);

  useEffect(() => {
    void refreshVaultFiles();
  }, [refreshVaultFiles]);

  useEffect(() => {
    if (mentionOpen) void refreshVaultFiles();
  }, [mentionOpen, refreshVaultFiles]);

  useEffect(() => {
    return () => {
      loadSeqRef.current += 1;
    };
  }, [loadVaultFiles]);

  const syncMentionFromInput = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta) {
      setMentionOpen(false);
      return;
    }
    const active = findActiveMentionQuery(input, ta.selectionStart);
    if (active) {
      setMentionOpen(true);
      setMentionStart(active.start);
      setMentionQuery(active.query);
      setMentionPrefix(active.prefix);
    } else {
      setMentionOpen(false);
    }
  }, [input, textareaRef]);

  useEffect(() => {
    syncMentionFromInput();
  }, [input, syncMentionFromInput]);

  const selectMention = useCallback(
    (candidate: MentionCandidate) => {
      const ta = textareaRef.current;
      const cursor = ta?.selectionStart ?? input.length;
      const next = insertMentionToken(input, cursor, mentionStart, candidate);
      setInput(next.text);
      setMentionOpen(false);
      requestAnimationFrame(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.focus();
        el.setSelectionRange(next.cursor, next.cursor);
      });
    },
    [input, mentionStart, setInput, textareaRef],
  );

  const removeMentionToken = useCallback(
    (token: MentionToken) => {
      setInput((prev) => prev.replace(token.raw, "").replace(/\s{2,}/g, " "));
    },
    [setInput],
  );

  const {
    highlight: mentionHighlight,
    handleKeyDown: handleMentionKeyDown,
    setHighlight: setMentionHighlight,
    navDeltaRef: mentionNavDeltaRef,
  } = useListboxKeyboard({
    length: mentionCandidates.length,
    enabled: mentionOpen && mentionCandidates.length > 0,
    wrap: false,
    resetKey: `${mentionQuery}:${mentionCandidates.length}`,
    onActivate: (index) => {
      const item = mentionCandidates[index];
      if (item) selectMention(item);
    },
  });

  const handleComposerKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (mentionOpen) {
        if (event.key === "Escape") {
          event.preventDefault();
          setMentionOpen(false);
          return;
        }
        if (handleMentionKeyDown(event)) return;
      }
    },
    [handleMentionKeyDown, mentionOpen],
  );

  return {
    contextScope,
    handleComposerKeyDown,
    mentionCandidates,
    mentionHighlight,
    mentionNavDeltaRef,
    mentionOpen,
    mentionPrefix,
    mentionQuery,
    mentionTokens,
    removeMentionToken,
    selectMention,
    setMentionHighlight,
    syncMentionFromInput,
  };
}
