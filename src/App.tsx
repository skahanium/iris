import AppImpl from "./App.impl";

/*
Source contract anchors for the thin App facade.

<MinimalWindowChrome />
UnifiedAssistantPanel
useEditorStats
editorStats
onBodyStatsChange
const md = await flushSave();
versionSaveManual(path, md)
setMarkdown(md)
persistActiveTabBeforeLeave
flushSaveForPath
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
绗旇宸查攣瀹氾紝鏃犳硶淇濆瓨
笔记已锁定，无法保存
const assistantNotePath = activeNoteIsClassified ? null : activePath;
notePath={assistantNotePath}
getNoteContent={getLiveMarkdown}
if (isClassifiedVaultPath(path)) return null;
if (activeNoteIsClassified) {
娑夊瘑绗旇涓嶈兘鍙戦€佸埌 AI
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
homeActive
AiSystemCenterPanel
data-testid="editor-shell"
runEditorAction
IrisContextMenu
onBodyContextMenu
useEditorContextMenu
*/

export default AppImpl;
