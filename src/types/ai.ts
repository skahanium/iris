// AI Runtime core types — mirrors Rust ai_runtime::types

export type AiScene =
  | "knowledge_lookup"
  | "exemplar_learning"
  | "drafting_assist"
  | "research_synthesis";

export type AutonomyLevel = "L0" | "L1" | "L2" | "L3";

export type SourceType =
  | "note"
  | "anchor"
  | "regulation"
  | "template"
  | "session"
  | "web";

export type TrustLevel =
  | "user_note"
  | "derived_cache"
  | "external_web"
  | "model_generated";

export type ToolAccessLevel =
  | "read_index"
  | "read_note_span"
  | "read_profile"
  | "network"
  | "write_cache"
  | "write_markdown"
  | "write_settings";

export interface SourceSpan {
  start: number;
  end: number;
}

export interface ContextPacket {
  id: string;
  source_type: SourceType;
  source_path: string | null;
  title: string;
  heading_path: string | null;
  source_span: SourceSpan | null;
  content_hash: string;
  excerpt: string;
  retrieval_reason: string;
  score: number;
  trust_level: TrustLevel;
  citation_label: string;
  stale: boolean;
}

export interface ToolSpec {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  access_level: ToolAccessLevel;
  scene_allowlist: AiScene[];
  requires_confirmation: boolean;
  max_results: number | null;
}

/** Retrieval scope from `@` mentions (IPC camelCase). */
export interface ContextScope {
  paths: string[];
  pathPrefixes: string[];
  corpusIds?: string[];
}

export interface ContextStatus {
  regulations_loaded: number;
  model_essays_loaded: number;
  anchors_loaded: number;
  links_loaded: number;
  total_tokens_estimate: number;
}

export interface AssembledContext {
  packets: ContextPacket[];
  tools: ToolSpec[];
  context_status: ContextStatus;
}

export interface ToolConfirmRequest {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}

// Scene display metadata
export interface SceneMeta {
  scene: AiScene;
  label: string;
  description: string;
  icon: string;
  defaultScope: "global" | "document";
}
