export const IPC_EVENTS = {
  VERSION_SAVE_COMPLETE: "version:save_complete",
  FILE_CHANGED: "file:changed",
  CLASSIFIED_FILE_TAKEN: "classified:file_taken",
  SKILLS_CHANGED: "skills:changed",
  ASSISTANT_RUN_EVENT: "assistant:run_event",
  ASSISTANT_RUN_PRESENTATION: "assistant:run_presentation",
  EMBEDDING_INDEX_PROGRESS: "embedding-index-progress",
  APP_UPDATE_STATUS: "app-update:status",
  APP_UPDATE_PROGRESS: "app-update:progress",
} as const;

export type IpcEventName = (typeof IPC_EVENTS)[keyof typeof IPC_EVENTS];
