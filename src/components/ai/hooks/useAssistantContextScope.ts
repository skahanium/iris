import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";

import {
  buildMentionCandidates,
  mentionsToContextScope,
  type MentionTextEdit,
  type MentionCandidate,
} from "@/lib/ai-context-scope";
import { fileList, folderList, tagList } from "@/lib/ipc";
import type { DisplayMention, SecurityDomain } from "@/types/ai";
import type { FileListItem, TagGroup } from "@/types/ipc";
import type { AssistantComposerHandle } from "@/components/ui/ai-composer";

interface UseAssistantContextScopeOptions {
  setInput: Dispatch<SetStateAction<string>>;
  domain?: SecurityDomain;
  input?: string;
  textareaRef?: RefObject<HTMLTextAreaElement | null>;
  loadVaultFiles?: () => Promise<FileListItem[]>;
  loadVaultFolders?: () => Promise<string[]>;
  loadVaultTags?: () => Promise<TagGroup[]>;
  runtimeDocumentCandidates?: FileListItem[];
  composerRef?: RefObject<AssistantComposerHandle | null>;
}

export function useAssistantContextScope({
  setInput,
  domain = "normal",
  loadVaultFiles = fileList,
  loadVaultFolders = folderList,
  loadVaultTags = tagList,
  runtimeDocumentCandidates = [],
  composerRef,
}: UseAssistantContextScopeOptions) {
  const [vaultFiles, setVaultFiles] = useState<FileListItem[]>([]);
  const [vaultFolders, setVaultFolders] = useState<string[]>([]);
  const [vaultTags, setVaultTags] = useState<TagGroup[]>([]);
  const [displayMentions, setDisplayMentions] = useState<DisplayMention[]>([]);
  const loadSeqRef = useRef(0);
  const domainRef = useRef(domain);
  domainRef.current = domain;
  const displayMentionsByDomainRef = useRef<
    Partial<Record<SecurityDomain, DisplayMention[]>>
  >({ normal: [], classified: [] });

  useEffect(() => {
    setDisplayMentions(displayMentionsByDomainRef.current[domain] ?? []);
  }, [domain]);

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
  const getMentionCandidates = useCallback(
    (prefix: "@" | "#", query: string) =>
      buildMentionCandidates(mentionSourceFiles, query, {
        prefix,
        folderPrefixes: vaultFolders,
        tags: vaultTags,
      }),
    [mentionSourceFiles, vaultFolders, vaultTags],
  );

  const refreshMentionSources = useCallback(() => {
    const seq = loadSeqRef.current + 1;
    loadSeqRef.current = seq;
    return Promise.allSettled([
      loadVaultFiles(),
      loadVaultFolders(),
      loadVaultTags(),
    ]).then(([filesResult, foldersResult, tagsResult]) => {
      if (loadSeqRef.current === seq) {
        setVaultFiles(
          filesResult.status === "fulfilled" ? filesResult.value : [],
        );
        setVaultFolders(
          foldersResult.status === "fulfilled" ? foldersResult.value : [],
        );
        setVaultTags(tagsResult.status === "fulfilled" ? tagsResult.value : []);
      }
    });
  }, [loadVaultFiles, loadVaultFolders, loadVaultTags]);

  useEffect(() => {
    void refreshMentionSources();
  }, [refreshMentionSources]);

  useEffect(() => {
    return () => {
      loadSeqRef.current += 1;
    };
  }, [loadVaultFiles, loadVaultFolders, loadVaultTags]);

  const commitDisplayMentions = useCallback((mentions: DisplayMention[]) => {
    displayMentionsByDomainRef.current[domainRef.current] = mentions;
    setDisplayMentions(mentions);
  }, []);

  const handleInputChange = useCallback(
    (
      nextInput: string,
      mentionsOrEdit: DisplayMention[] | MentionTextEdit = [],
    ) => {
      const mentions = Array.isArray(mentionsOrEdit) ? mentionsOrEdit : [];
      commitDisplayMentions(mentions);
      setInput(nextInput);
    },
    [commitDisplayMentions, setInput],
  );

  const selectMention = useCallback(
    (candidate: MentionCandidate) => {
      composerRef?.current?.insertMention(candidate);
    },
    [composerRef],
  );

  return {
    displayMentions,
    getMentionCandidates,
    handleInputChange,
    retrievalScope,
    selectMention,
  };
}
