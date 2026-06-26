export const IPC_EVENTS = {
  VERSION_SAVE_COMPLETE: "version:save_complete",
  FILE_CHANGED: "file:changed",
  CLASSIFIED_FILE_TAKEN: "classified:file_taken",
  SKILLS_CHANGED: "skills:changed",
  TOOL_CONFIRM_REQUEST: "ai:tool_confirm_request",
  LLM_TOKEN: "llm:token",
  LLM_DONE: "llm:done",
  LLM_ERROR: "llm:error",
  AI_RETRY_STATUS: "ai:retry_status",
  HARNESS_TRACE: "ai:harness_trace",
  AI_THINKING: "ai:thinking",
  AI_REQUEST_STARTED: "ai:request_started",
  RESEARCH_PROGRESS: "ai:research_progress",
} as const;

export type IpcEventName = (typeof IPC_EVENTS)[keyof typeof IPC_EVENTS];
