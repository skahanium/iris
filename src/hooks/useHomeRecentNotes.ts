import { useCallback, useEffect, useRef, useState } from "react";

import { fileList } from "@/lib/ipc";
import type { NoteOpenSource } from "@/lib/document-open-runtime";
import type { FileListItem } from "@/types/ipc";

interface UseHomeRecentNotesOptions {
  enabled: boolean;
  onPrepare?: (file: FileListItem, source: NoteOpenSource) => void;
  vaultIndexEpoch: number;
  vaultPath: string | null;
}

interface UseHomeRecentNotesResult {
  catalogPaths: readonly string[];
  recentNotes: readonly FileListItem[];
  vaultHasNotes: boolean;
  refreshRecent: () => Promise<void>;
}

function dedupeByPath(files: FileListItem[]): FileListItem[] {
  const byPath = new Map<string, FileListItem>();
  for (const file of files) {
    if (!byPath.has(file.path)) {
      byPath.set(file.path, file);
    }
  }
  return [...byPath.values()];
}

export function useHomeRecentNotes({
  enabled,
  onPrepare,
  vaultIndexEpoch,
  vaultPath,
}: UseHomeRecentNotesOptions): UseHomeRecentNotesResult {
  const [catalogFiles, setCatalogFiles] = useState<FileListItem[]>([]);
  const [recentNotes, setRecentNotes] = useState<FileListItem[]>([]);
  const requestSequenceRef = useRef(0);
  const vaultPathRef = useRef(vaultPath);
  const previousVaultPathRef = useRef(vaultPath);
  vaultPathRef.current = vaultPath;

  const refreshRecent = useCallback(async () => {
    const requestVaultPath = vaultPathRef.current;
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;

    if (!requestVaultPath) {
      setCatalogFiles([]);
      setRecentNotes([]);
      return;
    }

    try {
      const files = await fileList();
      if (
        requestSequenceRef.current !== requestSequence ||
        vaultPathRef.current !== requestVaultPath
      ) {
        return;
      }
      const catalog = dedupeByPath(files);
      setCatalogFiles(catalog);
      setRecentNotes(catalog.slice(0, 5));
    } catch (error) {
      console.warn("[WorkspaceEmpty] catalog refresh failed:", error);
    }
  }, []);

  useEffect(() => {
    const previousVaultPath = previousVaultPathRef.current;
    vaultPathRef.current = vaultPath;
    if (previousVaultPath !== vaultPath) {
      requestSequenceRef.current += 1;
      setCatalogFiles([]);
      setRecentNotes([]);
      previousVaultPathRef.current = vaultPath;
    }
    if (enabled) {
      void refreshRecent();
    }
  }, [enabled, refreshRecent, vaultIndexEpoch, vaultPath]);

  useEffect(() => {
    if (enabled) {
      recentNotes.forEach((file) => onPrepare?.(file, "workspace_empty"));
    }
  }, [enabled, onPrepare, recentNotes]);

  return {
    catalogPaths: catalogFiles.map((file) => file.path),
    recentNotes,
    vaultHasNotes: catalogFiles.length > 0,
    refreshRecent,
  };
}
