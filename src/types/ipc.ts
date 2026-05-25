export interface FileListItem {
  path: string;
  title: string;
  updated_at: string;
}

export interface FileEntry {
  id: number;
  path: string;
  title: string;
  updated_at: string;
  word_count: number;
}

export interface ChatMessage {
  role: string;
  content: string;
}

export interface LlmGenerateParams {
  provider: string;
  model?: string;
  messages: ChatMessage[];
  system?: string;
  stream?: boolean;
  custom_base_url?: string;
  web_search?: boolean;
}

export interface LlmProviderInfo {
  id: string;
  name: string;
  default_model: string;
}

export interface KeywordHit {
  path: string;
  title: string;
  snippet: string;
}

export interface SemanticHit {
  chunk_id: number;
  path: string;
  title: string;
  snippet: string;
  score: number;
}

export interface FileChangedEvent {
  path: string;
  hash?: string;
  event_type: string;
}

export interface LlmTokenEvent {
  request_id: string;
  token: string;
  index: number;
}

export interface BacklinkEntry {
  source_path: string;
  source_title: string;
  context: string | null;
}

export interface GraphNode {
  id: number;
  path: string;
  title: string;
  link_count: number;
}

export interface GraphEdge {
  source: number;
  target: number;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface VersionEntry {
  id: number;
  file_id: number;
  version_no: string;
  label: string | null;
  content_hash: string;
  word_count: number;
  is_finalized: boolean;
  created_at: string;
}
