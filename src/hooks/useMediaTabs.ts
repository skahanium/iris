import { useCallback, useMemo, useState } from "react";

import { classifyWorkspacePath } from "@/lib/media-reference";
import type { WorkspaceMediaKind } from "@/types/ipc";

type ConcreteMediaKind = Exclude<WorkspaceMediaKind, null>;

export interface MediaTab {
  id: string;
  mediaKind: ConcreteMediaKind;
  mimeType: string | null;
  path: string;
  sizeBytes: number | null;
  title: string;
  updatedAt: string | null;
}

export function mediaTabId(path: string): string {
  return `media:${path}`;
}

function mediaTitle(path: string, titleHint?: string): string {
  if (titleHint?.trim()) return titleHint.trim();
  const name = path.split("/").pop() ?? path;
  return name.replace(/\.[^.]+$/, "") || name;
}

function mediaMimeType(
  mediaKind: ConcreteMediaKind,
  path: string,
): string | null {
  const ext = path.split(".").pop()?.toLowerCase();
  if (mediaKind === "pdf") return "application/pdf";
  if (mediaKind === "image") {
    if (ext === "jpg") return "image/jpeg";
    return ext ? `image/${ext}` : null;
  }
  if (mediaKind === "video") {
    if (ext === "mov") return "video/quicktime";
    if (ext === "m4v") return "video/x-m4v";
    return ext ? `video/${ext}` : null;
  }
  return null;
}

export function useMediaTabs() {
  const [mediaTabs, setMediaTabs] = useState<MediaTab[]>([]);
  const [activeMediaId, setActiveMediaId] = useState<string | null>(null);

  const openMediaPath = useCallback((path: string, titleHint?: string) => {
    const classification = classifyWorkspacePath(path);
    if (classification.kind !== "media" || !classification.mediaKind) {
      return false;
    }
    const tab: MediaTab = {
      id: mediaTabId(path),
      mediaKind: classification.mediaKind,
      mimeType: mediaMimeType(classification.mediaKind, path),
      path,
      sizeBytes: null,
      title: mediaTitle(path, titleHint),
      updatedAt: null,
    };
    setMediaTabs((prev) => {
      const next = [...prev.filter((item) => item.id !== tab.id), tab];
      return next.slice(-10);
    });
    setActiveMediaId(tab.id);
    return true;
  }, []);

  const activateMedia = useCallback((id: string) => {
    setActiveMediaId(id);
  }, []);

  const closeMedia = useCallback((id: string) => {
    setMediaTabs((prev) => prev.filter((item) => item.id !== id));
    setActiveMediaId((current) => (current === id ? null : current));
  }, []);

  const activeMediaTab = useMemo(
    () => mediaTabs.find((item) => item.id === activeMediaId) ?? null,
    [activeMediaId, mediaTabs],
  );

  return {
    activateMedia,
    activeMediaId,
    activeMediaTab,
    closeMedia,
    mediaTabs,
    openMediaPath,
    setActiveMediaId,
  };
}
