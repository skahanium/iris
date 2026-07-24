import AppImpl from "./App.impl";

/*
Source contract anchors for the thin App facade.

<MinimalWindowChrome />
UnifiedAssistantPanel
useEditorStats
editorStats
onBodyStatsChange
const md = await flushSave();
versionSnapshotScheduler.saveManual(path, md)
setMarkdown(md)
DocumentPersistenceCoordinator
flushAllOpenTabs
reason: "app_close"
setAppClosing(true)
clearVersionIdleTimer
onThemeChange={(nextTheme) => void setTheme(nextTheme)}
.then(({ content: externalContent })
fileSetLock
ClassifiedPanel
classifiedOpen
listenClassifiedFileTaken
locked={
setLocked={
useClassifiedVaultSession
activeNoteIsClassified
笔记已锁定，无法保存
const assistantNotePath = activeNoteIsClassified ? null : activePath;
notePath={assistantNotePath}
getNoteContent={getLiveMarkdown}
if (isClassifiedVaultPath(path)) return null;
if (activeNoteIsClassified) {
涉密笔记不能发送到 AI
onOpenFile={(path) =>
openNoteLeavingHome(path, undefined, { allowClassified: true })
aiPanelOpen
scheduleUndoRedoStateRefresh
requestAnimationFrame
handleUndo
scheduleUndoRedoStateRefresh
handleRedo
scheduleUndoRedoStateRefresh
workspaceEmpty
ManagementCenterPanel
data-testid="editor-shell"
runEditorAction
IrisContextMenu
onBodyContextMenu
useEditorContextMenu
*/

export default AppImpl;
