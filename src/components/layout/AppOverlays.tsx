import { lazy, Suspense } from "react";

import { SkillsPanel } from "@/components/ai/SkillsPanel";
import { ClassifiedPanel } from "@/components/classified/ClassifiedPanel";
import { BacklinksPanel } from "@/components/file/BacklinksPanel";
import { ConflictDialog } from "@/components/file/ConflictDialog";
import { VaultNavigator } from "@/components/file/VaultNavigator";
import { QuickOpen } from "@/components/file/QuickOpen";
import { RecycleBinSheet } from "@/components/file/RecycleBinSheet";
import { SearchPanel } from "@/components/file/SearchPanel";
import { TagView } from "@/components/tag/TagView";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { CommandPalette } from "@/components/layout/CommandPalette";
import type { CommandPaletteItem } from "@/lib/command-palette";
import type { OverlayId } from "@/hooks/useOverlayManager";
import type { TabItem } from "@/components/layout/TabBar";
import type { ClassifiedStatus } from "@/types/ipc";

const GraphView = lazy(() =>
  import("@/components/graph/GraphView").then((m) => ({
    default: m.GraphView,
  })),
);
const SettingsPanel = lazy(() =>
  import("@/components/settings/SettingsPanel").then((m) => ({
    default: m.SettingsPanel,
  })),
);
const AiSystemCenterPanel = lazy(() =>
  import("@/components/settings/AiSystemCenterPanel").then((m) => ({
    default: m.AiSystemCenterPanel,
  })),
);
const VersionTimeline = lazy(() =>
  import("@/components/version/VersionTimeline").then((m) => ({
    default: m.VersionTimeline,
  })),
);

const LazyFallback = () => (
  <div className="flex items-center justify-center p-8 text-sm text-muted-foreground">
    加载中…
  </div>
);

interface OverlayPort {
  commandPaletteOpen: boolean;
  quickOpen: boolean;
  fileSheet: boolean;
  recycleBinOpen: boolean;
  searchOpen: boolean;
  settingsOpen: boolean;
  aiSystemCenterOpen: boolean;
  skillsOpen: boolean;
  backlinksOpen: boolean;
  tagViewOpen: boolean;
  versionOpen: boolean;
  graphOpen: boolean;
  closeOverlay: (overlay?: OverlayId) => void;
}

interface ConflictState {
  open: boolean;
  localContent: string;
  externalContent: string;
  filePath: string;
}

interface VersionSchedulerPort {
  markHighPriorityStart: (path: string) => void;
  markHighPriorityEnd: (path: string) => void;
}

interface AppOverlaysProps {
  activePath: string | null;
  applyMarkdownToEditor: (content: string) => void;
  bumpVaultIndex: () => void;
  classifiedIdleDeadline: number | null;
  classifiedOpen: boolean;
  classifiedVaultStatus: ClassifiedStatus;
  classifiedWaiting: boolean;
  commandPaletteItems: CommandPaletteItem[];
  conflictState: ConflictState | null;
  getCurrentContent: () => string;
  handleCommandPaletteSelect: (item: CommandPaletteItem) => void;
  handleConflictAcceptExternal: () => void;
  handleConflictKeepLocal: () => void;
  handleConflictManualEdit: () => void;
  markdown: string;
  onClassifiedUnlocked: () => void;
  openClassifiedPaths: string[];
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: { allowClassified?: boolean },
  ) => void;
  overlays: OverlayPort;
  refreshClassifiedStatus: () => Promise<ClassifiedStatus>;
  requestClassifiedLock: () => Promise<boolean>;
  setClassifiedOpen: (open: boolean) => void;
  setClassifiedWaiting: (waiting: boolean) => void;
  setTheme: (theme: "dark" | "light") => Promise<void>;
  setWebSearch: (enabled: boolean) => void;
  tabs: TabItem[];
  theme: "dark" | "light";
  touchClassifiedActivity: () => void;
  versionSnapshotScheduler: VersionSchedulerPort;
  webSearch: boolean;
}

export function AppOverlays({
  activePath,
  applyMarkdownToEditor,
  bumpVaultIndex,
  classifiedIdleDeadline,
  classifiedOpen,
  classifiedVaultStatus,
  classifiedWaiting,
  commandPaletteItems,
  conflictState,
  getCurrentContent,
  handleCommandPaletteSelect,
  handleConflictAcceptExternal,
  handleConflictKeepLocal,
  handleConflictManualEdit,
  markdown,
  onClassifiedUnlocked,
  openClassifiedPaths,
  openNoteLeavingHome,
  overlays,
  refreshClassifiedStatus,
  requestClassifiedLock,
  setClassifiedOpen,
  setClassifiedWaiting,
  setTheme,
  setWebSearch,
  tabs,
  theme,
  touchClassifiedActivity,
  versionSnapshotScheduler,
  webSearch,
}: AppOverlaysProps) {
  return (
    <>
      <CommandPalette
        open={overlays.commandPaletteOpen}
        items={commandPaletteItems}
        onClose={() => overlays.closeOverlay("commandPalette")}
        onSelect={handleCommandPaletteSelect}
      />
      <QuickOpen
        open={overlays.quickOpen}
        onClose={() => overlays.closeOverlay("quickOpen")}
        onSelect={openNoteLeavingHome}
      />
      <VaultNavigator
        open={overlays.fileSheet}
        onClose={() => overlays.closeOverlay("fileSheet")}
        onOpen={openNoteLeavingHome}
      />
      <RecycleBinSheet
        open={overlays.recycleBinOpen}
        onClose={() => overlays.closeOverlay("recycleBin")}
        onRestored={openNoteLeavingHome}
        onIndexChange={bumpVaultIndex}
      />
      <SearchPanel
        open={overlays.searchOpen}
        onClose={() => overlays.closeOverlay("search")}
        onOpen={openNoteLeavingHome}
      />
      <Suspense fallback={<LazyFallback />}>
        <SettingsPanel
          open={overlays.settingsOpen}
          onClose={() => overlays.closeOverlay("settings")}
          theme={theme}
          onThemeChange={(t) => void setTheme(t)}
          webSearch={webSearch}
          onWebSearchChange={setWebSearch}
        />
      </Suspense>
      <Suspense fallback={<LazyFallback />}>
        <AiSystemCenterPanel
          open={overlays.aiSystemCenterOpen}
          onClose={() => overlays.closeOverlay("aiSystemCenter")}
        />
      </Suspense>
      <SkillsPanel
        open={overlays.skillsOpen}
        onClose={() => overlays.closeOverlay("skills")}
      />
      <BacklinksPanel
        open={overlays.backlinksOpen}
        onClose={() => overlays.closeOverlay("backlinks")}
        notePath={activePath}
        onOpen={openNoteLeavingHome}
      />
      <TagView
        open={overlays.tagViewOpen}
        onClose={() => overlays.closeOverlay("tags")}
        onOpen={openNoteLeavingHome}
      />
      <Suspense fallback={<LazyFallback />}>
        <VersionTimeline
          open={overlays.versionOpen}
          onClose={() => overlays.closeOverlay("version")}
          notePath={activePath}
          currentContent={markdown}
          getCurrentContent={getCurrentContent}
          hasUnsavedEdits={
            tabs.find((tab) => tab.path === activePath)?.dirty ?? false
          }
          onRestore={applyMarkdownToEditor}
          onHighPriorityStart={(path) =>
            versionSnapshotScheduler.markHighPriorityStart(path)
          }
          onHighPriorityEnd={(path) =>
            versionSnapshotScheduler.markHighPriorityEnd(path)
          }
        />
      </Suspense>
      <ErrorBoundary scope="知识图谱">
        <Suspense fallback={<LazyFallback />}>
          <GraphView
            open={overlays.graphOpen}
            onClose={() => overlays.closeOverlay("graph")}
            onOpenNote={openNoteLeavingHome}
          />
        </Suspense>
      </ErrorBoundary>
      <ConflictDialog
        open={conflictState?.open ?? false}
        localContent={conflictState?.localContent ?? ""}
        externalContent={conflictState?.externalContent ?? ""}
        filePath={conflictState?.filePath ?? ""}
        onKeepLocal={handleConflictKeepLocal}
        onAcceptExternal={handleConflictAcceptExternal}
        onManualEdit={handleConflictManualEdit}
      />
      <ClassifiedPanel
        open={classifiedOpen}
        onClose={() => setClassifiedOpen(false)}
        status={classifiedVaultStatus}
        waiting={classifiedWaiting}
        idleDeadline={classifiedIdleDeadline}
        openClassifiedPaths={openClassifiedPaths}
        onOpenFile={(path) =>
          openNoteLeavingHome(path, undefined, { allowClassified: true })
        }
        onUnlockSuccess={() => void onClassifiedUnlocked()}
        onRequestLock={() => requestClassifiedLock()}
        onActivity={touchClassifiedActivity}
        onRefreshStatus={refreshClassifiedStatus}
        onEnterWaiting={() => setClassifiedWaiting(true)}
      />
    </>
  );
}
