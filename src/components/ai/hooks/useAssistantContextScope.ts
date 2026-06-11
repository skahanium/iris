import {
  useCallback,
  useEffect,
  useMemo,
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
}

export function useAssistantContextScope({
  input,
  setInput,
  textareaRef,
  loadVaultFiles = fileList,
}: UseAssistantContextScopeOptions) {
  const [vaultFiles, setVaultFiles] = useState<FileListItem[]>([]);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionStart, setMentionStart] = useState(0);
  const [mentionQuery, setMentionQuery] = useState("");

  const mentionTokens = useMemo(() => parseMentionTokens(input), [input]);
  const contextScope = useMemo(
    () => tokensToContextScope(mentionTokens),
    [mentionTokens],
  );
  const mentionCandidates = useMemo(
    () => (mentionOpen ? buildMentionCandidates(vaultFiles, mentionQuery) : []),
    [mentionOpen, vaultFiles, mentionQuery],
  );

  useEffect(() => {
    let active = true;
    void loadVaultFiles()
      .then((files) => {
        if (active) setVaultFiles(files);
      })
      .catch(() => {
        if (active) setVaultFiles([]);
      });
    return () => {
      active = false;
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
    mentionQuery,
    mentionTokens,
    removeMentionToken,
    selectMention,
    setMentionHighlight,
    syncMentionFromInput,
  };
}
