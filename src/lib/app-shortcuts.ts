import type {
  ManagementCenterDetail,
  ManagementCenterSection,
  OverlayId,
} from "@/hooks/useOverlayManager";
import type { KeyChord } from "@/lib/utils";

export type AppShortcutAction =
  | { type: "openOverlay"; overlay: OverlayId }
  | {
      type: "openManagementCenter";
      section: ManagementCenterSection;
      detail?: ManagementCenterDetail;
    }
  | { type: "openClassifiedPanel" }
  | { type: "openFindReplace"; mode: "find" | "replace" }
  | { type: "newNote" }
  | { type: "saveNote" }
  | { type: "closeTab" }
  | { type: "toggleAiPanel" }
  | { type: "toggleZen" }
  | { type: "toggleOutline" }
  | { type: "toggleTheme" }
  | { type: "toggleWebSearch" }
  | { type: "rescanVault" }
  | { type: "zoomIn" }
  | { type: "zoomOut" }
  | { type: "zoomReset" }
  | { type: "sendSelectionToAi" };

export interface AppShortcutItem {
  id: string;
  disabled?: boolean;
  chord?: KeyChord;
  action: AppShortcutAction;
}

export interface AppShortcutContext {
  hasVault: boolean;
  hasActiveNote: boolean;
}

export function buildAppShortcutItems(
  ctx: AppShortcutContext,
): AppShortcutItem[] {
  const noteOnly = !ctx.hasActiveNote;
  const vaultOnly = !ctx.hasVault;

  return [
    {
      id: "quick-open",
      disabled: vaultOnly,
      chord: { key: "P", mod: true, requireVault: true },
      action: { type: "openOverlay", overlay: "quickOpen" },
    },
    {
      id: "file-sheet",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "fileSheet" },
    },
    {
      id: "recycle-bin",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "recycleBin" },
    },
    {
      id: "classified-panel",
      disabled: vaultOnly,
      chord: { key: "L", mod: true, shift: true, requireVault: true },
      action: { type: "openClassifiedPanel" },
    },
    {
      id: "search",
      disabled: vaultOnly,
      chord: { key: "F", mod: true, shift: true, requireVault: true },
      action: { type: "openOverlay", overlay: "search" },
    },
    {
      id: "document-find",
      disabled: noteOnly,
      chord: { key: "F", mod: true, requireNote: true },
      action: { type: "openFindReplace", mode: "find" },
    },
    {
      id: "document-replace",
      disabled: noteOnly,
      chord: { key: "H", mod: true, requireNote: true },
      action: { type: "openFindReplace", mode: "replace" },
    },
    {
      id: "knowledge-relations",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "knowledgeRelations" },
    },
    {
      id: "graph",
      disabled: vaultOnly,
      action: { type: "openOverlay", overlay: "graph" },
    },
    {
      id: "rescan-vault",
      disabled: vaultOnly,
      action: { type: "rescanVault" },
    },
    {
      id: "new-note",
      disabled: vaultOnly,
      action: { type: "newNote" },
    },
    {
      id: "save-note",
      disabled: vaultOnly || noteOnly,
      chord: { key: "S", mod: true, requireNote: true },
      action: { type: "saveNote" },
    },
    {
      id: "close-tab",
      disabled: noteOnly,
      chord: { key: "W", mod: true, requireNote: true },
      action: { type: "closeTab" },
    },
    {
      id: "version",
      disabled: vaultOnly || noteOnly,
      chord: { key: "V", mod: true, shift: true, requireNote: true },
      action: { type: "openOverlay", overlay: "version" },
    },
    {
      id: "toggle-outline",
      disabled: noteOnly,
      action: { type: "toggleOutline" },
    },
    {
      id: "toggle-zen",
      chord: { key: ".", mod: true },
      action: { type: "toggleZen" },
    },
    {
      id: "toggle-theme",
      action: { type: "toggleTheme" },
    },
    {
      id: "management-center",
      chord: { key: ",", mod: true },
      action: { type: "openManagementCenter", section: "overview" },
    },
    {
      id: "zoom-in",
      disabled: noteOnly,
      chord: { key: "+", mod: true, requireNote: true },
      action: { type: "zoomIn" },
    },
    {
      id: "zoom-out",
      disabled: noteOnly,
      chord: { key: "-", mod: true, requireNote: true },
      action: { type: "zoomOut" },
    },
    {
      id: "zoom-reset",
      disabled: noteOnly,
      chord: { key: "0", mod: true, requireNote: true },
      action: { type: "zoomReset" },
    },
    {
      id: "toggle-ai",
      chord: { key: "A", mod: true, shift: true },
      action: { type: "toggleAiPanel" },
    },
    {
      id: "send-selection-ai",
      disabled: noteOnly,
      action: { type: "sendSelectionToAi" },
    },
    {
      id: "toggle-web-search",
      action: { type: "toggleWebSearch" },
    },
    {
      id: "ai-system-center",
      action: { type: "openManagementCenter", section: "ai" },
    },
    {
      id: "skills",
      action: { type: "openManagementCenter", section: "ai" },
    },
  ];
}
