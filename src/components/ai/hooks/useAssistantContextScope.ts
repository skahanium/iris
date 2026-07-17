import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent,
  type CompositionEvent,
  type RefObject,
  type SetStateAction,
} from "react";

import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import {
  buildMentionCandidates,
  findActiveMentionQuery,
  insertDisplayMention,
  mentionsToContextScope,
  reconcileDisplayMentions,
  validDisplayMentions,
  type MentionCandidate,
} from "@/lib/ai-context-scope";
import { fileList, tagList } from "@/lib/ipc";
import type { DisplayMention } from "@/types/ai";
import type { FileListItem, TagGroup } from "@/types/ipc";

interface UseAssistantContextScopeOptions {
  input: string;
  setInput: Dispatch<SetStateAction<string>>;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
  loadVaultFiles?: () => Promise<FileListItem[]>;
  loadVaultTags?: () => Promise<TagGroup[]>;
  runtimeDocumentCandidates?: FileListItem[];
}

export function useAssistantContextScope({
  input,
  setInput,
  textareaRef,
  loadVaultFiles = fileList,
  loadVaultTags = tagList,
  runtimeDocumentCandidates = [],
}: UseAssistantContextScopeOptions) {
  const [vaultFiles, setVaultFiles] = useState<FileListItem[]>([]);
  const [vaultTags, setVaultTags] = useState<TagGroup[]>([]);
  const [displayMentions, setDisplayMentions] = useState<DisplayMention[]>([]);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [mentionStart, setMentionStart] = useState(0);
  const [mentionQuery, setMentionQuery] = useState("");
  const [mentionPrefix, setMentionPrefix] = useState<"@" | "#">("@");
  const loadSeqRef = useRef(0);
  const previousInputRef = useRef(input);
  const displayMentionsRef = useRef<DisplayMention[]>([]);
  const composingRef = useRef(false);

  const retrievalScope = useMemo(
    () => mentionsToContextScope(displayMentions),
    [displayMentions],
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
        ? buildMentionCandidates(mentionSourceFiles, mentionQuery, {
            prefix: mentionPrefix,
            tags: vaultTags,
          })
        : [],
    [mentionOpen, mentionPrefix, mentionQuery, mentionSourceFiles, vaultTags],
  );

  const refreshMentionSources = useCallback(() => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    return Promise.allSettled([loadVaultFiles(), loadVaultTags()]).then(
      ([filesResult, tagsResult]) => {
        if (loadSeqRef.current === seq) {
          setVaultFiles(
            filesResult.status === "fulfilled" ? filesResult.value : [],
          );
          setVaultTags(
            tagsResult.status === "fulfilled" ? tagsResult.value : [],
          );
        }
      },
    );
  }, [loadVaultFiles, loadVaultTags]);

  useEffect(() => {
    void refreshMentionSources();
  }, [refreshMentionSources]);

  useEffect(() => {
    if (mentionOpen) void refreshMentionSources();
  }, [mentionOpen, refreshMentionSources]);

  useEffect(() => {
    return () => {
      loadSeqRef.current += 1;
    };
  }, [loadVaultFiles, loadVaultTags]);

  const commitDisplayMentions = useCallback((mentions: DisplayMention[]) => {
    displayMentionsRef.current = mentions;
    setDisplayMentions(mentions);
  }, []);

  useEffect(() => {
    const previous = previousInputRef.current;
    if (previous === input) return;
    const nextMentions = reconcileDisplayMentions(
      previous,
      input,
      displayMentionsRef.current,
    );
    previousInputRef.current = input;
    commitDisplayMentions(nextMentions);
  }, [commitDisplayMentions, input]);

  const handleInputChange = useCallback(
    (nextInput: string) => {
      const previous = previousInputRef.current;
      const nextMentions = reconcileDisplayMentions(
        previous,
        nextInput,
        displayMentionsRef.current,
      );
      previousInputRef.current = nextInput;
      commitDisplayMentions(nextMentions);
      setInput(nextInput);
    },
    [commitDisplayMentions, setInput],
  );

  const syncMentionFromInput = useCallback(() => {
    if (composingRef.current) {
      setMentionOpen(false);
      return;
    }
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
      const next = insertDisplayMention(input, cursor, mentionStart, candidate);
      const shifted = reconcileDisplayMentions(
        input,
        next.text,
        displayMentionsRef.current,
      );
      const nextMentions = validDisplayMentions(next.text, [
        ...shifted,
        next.mention,
      ]);
      previousInputRef.current = next.text;
      commitDisplayMentions(nextMentions);
      setInput(next.text);
      setMentionOpen(false);
      requestAnimationFrame(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.focus();
        el.setSelectionRange(next.cursor, next.cursor);
      });
    },
    [commitDisplayMentions, input, mentionStart, setInput, textareaRef],
  );

  const handleCompositionStart = useCallback(
    (_event: CompositionEvent<HTMLTextAreaElement>) => {
      composingRef.current = true;
      setMentionOpen(false);
    },
    [],
  );

  const handleCompositionEnd = useCallback(
    (_event: CompositionEvent<HTMLTextAreaElement>) => {
      composingRef.current = false;
      requestAnimationFrame(syncMentionFromInput);
    },
    [syncMentionFromInput],
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
    displayMentions,
    handleCompositionEnd,
    handleCompositionStart,
    handleComposerKeyDown,
    handleInputChange,
    mentionCandidates,
    mentionHighlight,
    mentionNavDeltaRef,
    mentionOpen,
    mentionPrefix,
    mentionQuery,
    retrievalScope,
    selectMention,
    setMentionHighlight,
    syncMentionFromInput,
  };
}
