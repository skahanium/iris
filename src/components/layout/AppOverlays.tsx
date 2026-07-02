import { lazy, Suspense } from "react";

import { ClassifiedPanel } from "@/components/classified/ClassifiedPanel";
import { ConflictDialog } from "@/components/file/ConflictDialog";
import { VaultNavigator } from "@/components/file/VaultNavigator";
import { QuickOpen } from "@/components/file/QuickOpen";
import { RecycleBinSheet } from "@/components/file/RecycleBinSheet";
import { SearchPanel } from "@/components/file/SearchPanel";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { KnowledgeRelationsPanel } from "@/components/knowledge/KnowledgeRelationsPanel";
import type {
  ManagementCenterDetail,
  ManagementCenterSection,
  OverlayId,
} from "@/hooks/useOverlayManager";
import type { TabItem } from "@/components/layout/TabBar";
import type {
  DocumentOpenPriority,
  NoteOpenBudgetKind,
  NoteOpenSource,
  PrepareNoteOpenRequest,
  PreparedNoteOpen,
} from "@/lib/document-open-runtime";
import type { ClassifiedStatus, FileListItem } from "@/types/ipc";
import type {
  WebSearchAvailability,
  WebSearchProviderOption,
} from "@/lib/web-search-provider-state";

const GraphView = lazy(() =>
  import("@/components/graph/GraphView").then((m) => ({
    default: m.GraphView,
  })),
);
const ManagementCenterPanel = lazy(() =>
  import("@/components/settings/ManagementCenterPanel").then((m) => ({
    default: m.ManagementCenterPanel,
  })),
);
const VersionTimeline = lazy(() =>
  import("@/components/version/VersionTimeline").then((m) => ({
    default: m.VersionTimeline,
  })),
);

interface OverlayPort {
  quickOpen: boolean;
  fileSheet: boolean;
  recycleBinOpen: boolean;
  searchOpen: boolean;
  managementCenterOpen: boolean;
  managementCenterSection: ManagementCenterSection;
  managementCenterDetail: ManagementCenterDetail;
  knowledgeRelationsOpen: boolean;
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
  autoVersionSettings: {
    autoVersionEnabled: boolean;
    autoVersionIdleMinutes: number;
    setAutoVersionEnabled: (enabled: boolean) => void;
    setAutoVersionIdleMinutes: (minutes: number) => void;
  };
  bumpVaultIndex: () => void;
  classifiedIdleDeadline: number | null;
  classifiedOpen: boolean;
  classifiedVaultStatus: ClassifiedStatus;
  classifiedWaiting: boolean;
  conflictState: ConflictState | null;
  getCurrentContent: () => string;
  onBeforeFinalizeCurrent: () => Promise<string | null>;
  handleConflictAcceptExternal: () => void;
  handleConflictKeepLocal: () => void;
  handleConflictManualEdit: () => void;
  markdown: string;
  onClassifiedUnlocked: () => void;
  onBeforeFilePathChange: (path: string) => Promise<void>;
  onFilePathChanged: (oldPath: string, newPath: string, title?: string) => void;
  onBeforeFileDelete: (path: string) => Promise<void>;
  onFileDeleted: (path: string) => void;
  openClassifiedPaths: string[];
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: {
      allowClassified?: boolean;
      openBudgetKind?: NoteOpenBudgetKind;
      openStartedAt?: number;
      openTraceRequest?: PrepareNoteOpenRequest;
      preparedNote?: PreparedNoteOpen;
      priority?: DocumentOpenPriority;
      source?: NoteOpenSource;
    },
  ) => void | Promise<void>;
  onPrepareNote?: (file: FileListItem, source?: NoteOpenSource) => void;
  onPrepareNotePath?: (
    path: string,
    titleHint?: string,
    source?: NoteOpenSource,
  ) => void;
  onPrepareClassifiedNotePath?: (
    path: string,
    titleHint?: string,
    source?: NoteOpenSource,
  ) => void;
  overlays: OverlayPort;
  refreshClassifiedStatus: () => Promise<ClassifiedStatus>;
  requestClassifiedLock: () => Promise<boolean>;
  setClassifiedOpen: (open: boolean) => void;
  setClassifiedWaiting: (waiting: boolean) => void;
  setWebSearch: (enabled: boolean) => void;
  webSearchAvailability: WebSearchAvailability;
  webSearchProviderId: string | null;
  webSearchProviders: WebSearchProviderOption[];
  setWebSearchProviderId: (providerId: string | null) => void;
  refreshWebSearchProviders: () => Promise<void>;
  openKnowledgeRelations: () => void;
  openVersion: () => void;
  rescanVault: () => void;
  tabs: TabItem[];
  touchClassifiedActivity: () => void;
  versionSnapshotScheduler: VersionSchedulerPort;
  webSearch: boolean;
}

export function AppOverlays({
  activePath,
  applyMarkdownToEditor,
  autoVersionSettings,
  bumpVaultIndex,
  classifiedIdleDeadline,
  classifiedOpen,
  classifiedVaultStatus,
  classifiedWaiting,
  conflictState,
  getCurrentContent,
  onBeforeFinalizeCurrent,
  handleConflictAcceptExternal,
  handleConflictKeepLocal,
  handleConflictManualEdit,
  markdown,
  onClassifiedUnlocked,
  onBeforeFilePathChange,
  onFilePathChanged,
  onBeforeFileDelete,
  onFileDeleted,
  openClassifiedPaths,
  openNoteLeavingHome,
  onPrepareNote,
  onPrepareNotePath,
  onPrepareClassifiedNotePath,
  overlays,
  refreshClassifiedStatus,
  requestClassifiedLock,
  setClassifiedOpen,
  setClassifiedWaiting,
  setWebSearch,
  webSearchAvailability,
  webSearchProviderId,
  webSearchProviders,
  setWebSearchProviderId,
  refreshWebSearchProviders,
  openKnowledgeRelations,
  openVersion,
  rescanVault,
  tabs,
  touchClassifiedActivity,
  versionSnapshotScheduler,
  webSearch,
}: AppOverlaysProps) {
  return (
    <>
      <QuickOpen
        open={overlays.quickOpen}
        onClose={() => overlays.closeOverlay("quickOpen")}
        onPrepare={(file, source) => onPrepareNote?.(file, source)}
        onSelect={(path, source) =>
          openNoteLeavingHome(path, undefined, {
            priority: "foreground",
            source,
          })
        }
      />
      <VaultNavigator
        open={overlays.fileSheet}
        onClose={() => overlays.closeOverlay("fileSheet")}
        onOpen={(path, source, options) =>
          openNoteLeavingHome(path, options?.titleHint, {
            ...options,
            priority: options?.priority ?? "foreground",
            source,
          })
        }
        onPrepare={(file, source) => onPrepareNote?.(file, source)}
        onBeforeFilePathChange={onBeforeFilePathChange}
        onFilePathChanged={onFilePathChanged}
        onBeforeFileDelete={onBeforeFileDelete}
        onFileDeleted={onFileDeleted}
      />
      <RecycleBinSheet
        open={overlays.recycleBinOpen}
        onClose={() => overlays.closeOverlay("recycleBin")}
        onRestored={(path) =>
          openNoteLeavingHome(path, undefined, {
            priority: "foreground",
            source: "recycle",
          })
        }
        onIndexChange={bumpVaultIndex}
      />
      <SearchPanel
        open={overlays.searchOpen}
        onClose={() => overlays.closeOverlay("search")}
        onOpen={(path) =>
          openNoteLeavingHome(path, undefined, {
            priority: "foreground",
            source: "search",
          })
        }
        onPrepare={(path, titleHint) =>
          onPrepareNotePath?.(path, titleHint, "search")
        }
      />
      {overlays.managementCenterOpen ? (
        <Suspense fallback={null}>
          <ManagementCenterPanel
            open={overlays.managementCenterOpen}
            onClose={() => overlays.closeOverlay("managementCenter")}
            section={overlays.managementCenterSection}
            detail={overlays.managementCenterDetail}
            webSearch={webSearch}
            webSearchAvailability={webSearchAvailability}
            webSearchProviderId={webSearchProviderId}
            webSearchProviders={webSearchProviders}
            onWebSearchChange={setWebSearch}
            onWebSearchProviderChange={setWebSearchProviderId}
            onRefreshWebSearchProviders={refreshWebSearchProviders}
            onOpenNote={(path) =>
              openNoteLeavingHome(path, undefined, {
                priority: "foreground",
                source: "management",
              })
            }
            onPrepareNote={(file) => onPrepareNote?.(file, "management")}
            onOpenKnowledgeRelations={openKnowledgeRelations}
            onOpenVersion={openVersion}
            onRescanVault={rescanVault}
            onRecycleIndexChange={bumpVaultIndex}
            onBeforeFilePathChange={onBeforeFilePathChange}
            onFilePathChanged={onFilePathChanged}
            onBeforeFileDelete={onBeforeFileDelete}
            onFileDeleted={onFileDeleted}
            onIndexChange={bumpVaultIndex}
            autoVersionEnabled={autoVersionSettings.autoVersionEnabled}
            autoVersionIdleMinutes={autoVersionSettings.autoVersionIdleMinutes}
            onAutoVersionEnabledChange={
              autoVersionSettings.setAutoVersionEnabled
            }
            onAutoVersionIdleMinutesChange={
              autoVersionSettings.setAutoVersionIdleMinutes
            }
          />
        </Suspense>
      ) : null}
      <KnowledgeRelationsPanel
        open={overlays.knowledgeRelationsOpen}
        onClose={() => overlays.closeOverlay("knowledgeRelations")}
        notePath={activePath}
        onOpen={(path) =>
          openNoteLeavingHome(path, undefined, {
            priority: "foreground",
            source: "link",
          })
        }
        onPreparePath={(path, titleHint) =>
          onPrepareNotePath?.(path, titleHint, "link")
        }
      />
      {overlays.versionOpen ? (
        <Suspense fallback={null}>
          <VersionTimeline
            open={overlays.versionOpen}
            onClose={() => overlays.closeOverlay("version")}
            notePath={activePath}
            currentContent={markdown}
            getCurrentContent={getCurrentContent}
            onBeforeFinalizeCurrent={onBeforeFinalizeCurrent}
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
      ) : null}
      {overlays.graphOpen ? (
        <ErrorBoundary scope="知识图谱">
          <Suspense fallback={null}>
            <GraphView
              open={overlays.graphOpen}
              onClose={() => overlays.closeOverlay("graph")}
              onOpenNote={(path) =>
                openNoteLeavingHome(path, undefined, {
                  priority: "foreground",
                  source: "graph",
                })
              }
              onPrepareNotePath={(path, titleHint) =>
                onPrepareNotePath?.(path, titleHint, "graph")
              }
            />
          </Suspense>
        </ErrorBoundary>
      ) : null}
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
          openNoteLeavingHome(path, undefined, {
            allowClassified: true,
            priority: "foreground",
            source: "classified",
          })
        }
        onPrepareFile={(path, titleHint) =>
          onPrepareClassifiedNotePath?.(path, titleHint, "classified")
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
