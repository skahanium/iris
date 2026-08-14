export const IPC_EVENTS = {
  FILE_CHANGED: "file:changed",
  CLASSIFIED_FILE_TAKEN: "classified:file_taken",
  ASSISTANT_RUN_EVENT: "assistant:run_event",
  ASSISTANT_RUN_PRESENTATION: "assistant:run_presentation",
  EMBEDDING_INDEX_PROGRESS: "embedding-index-progress",
  APP_UPDATE_STATUS: "app-update:status",
  APP_UPDATE_PROGRESS: "app-update:progress",
  FEED_CHANGED: "feed:changed",
  FEED_DOCUMENT_PROGRESS: "feed:document-progress",
} as const;

export type IpcEventName = (typeof IPC_EVENTS)[keyof typeof IPC_EVENTS];
