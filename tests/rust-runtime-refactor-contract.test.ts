import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function lineCount(path: string): number {
  return read(path).split("\n").length;
}

describe("Rust AI runtime refactor contract", () => {
  it("moves model gateway abort/messages/usage concerns behind dedicated modules", () => {
    const parent = read("src-tauri/src/ai_runtime/model_gateway_impl.rs");
    const abort = read("src-tauri/src/ai_runtime/model_gateway/abort.rs");
    const body = read("src-tauri/src/ai_runtime/model_gateway/body.rs");
    const httpBackend = read(
      "src-tauri/src/ai_runtime/model_gateway/http_backend.rs",
    );
    const messages = read("src-tauri/src/ai_runtime/model_gateway/messages.rs");
    const prompts = read("src-tauri/src/ai_runtime/model_gateway/prompts.rs");
    const streaming = read(
      "src-tauri/src/ai_runtime/model_gateway/streaming.rs",
    );
    const usage = read("src-tauri/src/ai_runtime/model_gateway/usage.rs");

    expect(parent).toContain("model_gateway/abort.rs");
    expect(parent).toContain("model_gateway/body.rs");
    expect(parent).toContain("model_gateway/http_backend.rs");
    expect(parent).toContain("model_gateway/messages.rs");
    expect(parent).toContain("model_gateway/prompts.rs");
    expect(parent).toContain("model_gateway/streaming.rs");
    expect(parent).toContain("model_gateway/usage.rs");
    expect(parent).not.toMatch(/^pub struct HttpLlmBackend/m);
    expect(parent).not.toMatch(/^pub fn build_drafting_prompt\(/m);
    expect(parent).not.toMatch(/^pub fn build_citation_prompt\(/m);
    expect(parent).not.toMatch(/^pub struct GatewayRequest/m);
    expect(parent).not.toMatch(/^pub fn build_chat_completions_body\(/m);
    expect(parent).not.toMatch(/^pub fn repair_tool_api_messages\(/m);
    expect(parent).not.toMatch(/^pub fn request_abort\(/m);
    expect(parent).not.toMatch(/^fn parse_usage\(/m);
    expect(parent).not.toMatch(/^pub struct StreamEvent/m);
    expect(parent).not.toMatch(/^pub async fn send_streaming_request\(/m);
    expect(abort).toContain("request_abort");
    expect(body).toContain("GatewayRequest");
    expect(body).toContain("build_chat_completions_body");
    expect(httpBackend).toContain("HttpLlmBackend");
    expect(httpBackend).toContain("format_llm_http_error");
    expect(messages).toContain("messages_for_api");
    expect(prompts).toContain("build_drafting_prompt");
    expect(streaming).toContain("StreamEvent");
    expect(streaming).toContain("send_streaming_request");
    expect(streaming).toContain("emit_stream_event");
    expect(usage).toContain("parse_usage");
  });

  it("keeps model_gateway_impl.rs below the current gateway split checkpoint", () => {
    expect(
      lineCount("src-tauri/src/ai_runtime/model_gateway_impl.rs"),
    ).toBeLessThanOrEqual(820);
  });

  it("moves retrieval broker exact/template/rank layers behind dedicated modules", () => {
    const parent = read("src-tauri/src/ai_runtime/retrieval_broker_impl.rs");
    const fts = read("src-tauri/src/ai_runtime/retrieval_broker/fts.rs");
    const graph = read("src-tauri/src/ai_runtime/retrieval_broker/graph.rs");
    const vector = read("src-tauri/src/ai_runtime/retrieval_broker/vector.rs");
    const exact = read("src-tauri/src/ai_runtime/retrieval_broker/exact.rs");
    const template = read(
      "src-tauri/src/ai_runtime/retrieval_broker/template.rs",
    );
    const rank = read("src-tauri/src/ai_runtime/retrieval_broker/rank.rs");

    expect(parent).toContain("retrieval_broker/fts.rs");
    expect(parent).toContain("retrieval_broker/graph.rs");
    expect(parent).toContain("retrieval_broker/vector.rs");
    expect(parent).toContain("retrieval_broker/exact.rs");
    expect(parent).toContain("retrieval_broker/template.rs");
    expect(parent).toContain("retrieval_broker/rank.rs");
    expect(parent).not.toContain("static RE_REGULATION_ARTICLE");
    expect(parent).not.toContain("fn search_fts");
    expect(parent).not.toContain("fn search_vector_chunks");
    expect(parent).not.toContain("fn search_graph_neighbors");
    expect(parent).not.toContain("fn search_template");
    expect(parent).not.toContain("fn fuse_and_rank");
    expect(parent).not.toContain("fn search_exact_regulation");
    expect(fts).toContain("search_fts");
    expect(graph).toContain("search_graph_neighbors");
    expect(vector).toContain("search_vector_chunks");
    expect(vector).toContain("search_vector_anchors");
    expect(vector).toContain("search_vector_regulations");
    expect(exact).toContain("search_exact_regulation");
    expect(template).toContain("search_template");
    expect(rank).toContain("fuse_and_rank");
  });

  it("keeps retrieval_broker_impl.rs below the current runtime split checkpoint", () => {
    expect(
      lineCount("src-tauri/src/ai_runtime/retrieval_broker_impl.rs"),
    ).toBeLessThanOrEqual(300);
  });

  it("moves foundational skills runtime concerns behind dedicated modules", () => {
    const parent = read("src-tauri/src/ai_runtime/skills_impl.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");
    const frontmatter = read("src-tauri/src/ai_runtime/skills/frontmatter.rs");
    const legacy = read("src-tauri/src/ai_runtime/skills/legacy.rs");
    const model = read("src-tauri/src/ai_runtime/skills/model.rs");
    const path = read("src-tauri/src/ai_runtime/skills/path.rs");
    const prompt = read("src-tauri/src/ai_runtime/skills/prompt.rs");
    const scan = read("src-tauri/src/ai_runtime/skills/scan.rs");
    const validation = read("src-tauri/src/ai_runtime/skills/validation.rs");

    expect(parent).toContain("skills/activation.rs");
    expect(parent).toContain("skills/frontmatter.rs");
    expect(parent).toContain("skills/legacy.rs");
    expect(parent).toContain("skills/model.rs");
    expect(parent).toContain("skills/path.rs");
    expect(parent).toContain("skills/prompt.rs");
    expect(parent).toContain("skills/scan.rs");
    expect(parent).toContain("skills/validation.rs");
    expect(parent).not.toMatch(/^fn parse_frontmatter\(/m);
    expect(parent).not.toMatch(/^fn global_skills_dir\(/m);
    expect(parent).not.toMatch(/^pub fn scan_all\(/m);
    expect(parent).not.toMatch(/^pub fn load_skill\(/m);
    expect(parent).not.toMatch(/^pub fn rank_skills_for_scene\(/m);
    expect(parent).not.toMatch(/^pub fn active_skills_for_prompt\(/m);
    expect(parent).not.toMatch(/^pub fn inject_into_prompt\(/m);
    expect(parent).not.toMatch(/^pub fn read_skill_resource\(/m);
    expect(parent).not.toMatch(/^pub fn migrate_legacy_skill\(/m);
    expect(parent).not.toMatch(/^pub fn validate_skill_license\(/m);
    expect(parent).not.toMatch(/^pub enum SkillScope/m);
    expect(activation).toContain("rank_skills_for_scene");
    expect(activation).toContain("active_skills_for_prompt");
    expect(frontmatter).toContain("parse_frontmatter");
    expect(legacy).toContain("migrate_legacy_skill");
    expect(model).toContain("SkillEntry");
    expect(path).toContain("global_skills_dir");
    expect(prompt).toContain("inject_into_prompt");
    expect(scan).toContain("scan_all_metadata");
    expect(scan).toContain("load_skill");
    expect(validation).toContain("validate_skill_license");
  });

  it("keeps skills_impl.rs below the current runtime split checkpoint", () => {
    expect(
      lineCount("src-tauri/src/ai_runtime/skills_impl.rs"),
    ).toBeLessThanOrEqual(1340);
  });

  it("moves tool catalog entry groups behind dedicated modules", () => {
    const parent = read("src-tauri/src/ai_runtime/tool_catalog_impl.rs");
    const groups = read("src-tauri/src/ai_runtime/tool_catalog/groups.rs");
    const readTools = read("src-tauri/src/ai_runtime/tool_catalog/read.rs");
    const root = read("src-tauri/src/ai_runtime/tool_catalog/root.rs");
    const skills = read("src-tauri/src/ai_runtime/tool_catalog/skills.rs");
    const web = read("src-tauri/src/ai_runtime/tool_catalog/web.rs");
    const write = read("src-tauri/src/ai_runtime/tool_catalog/write.rs");

    expect(parent).toContain("tool_catalog/groups.rs");
    expect(parent).toContain("tool_catalog/read.rs");
    expect(parent).toContain("tool_catalog/root.rs");
    expect(parent).toContain("tool_catalog/skills.rs");
    expect(parent).toContain("tool_catalog/web.rs");
    expect(parent).toContain("tool_catalog/write.rs");
    expect(parent).not.toContain('name: "search_hybrid"');
    expect(parent).not.toContain('name: "insert_text_at_cursor"');
    expect(parent).not.toContain('name: "skills_install"');
    expect(groups).toContain("collect_tool_catalog");
    expect(readTools).toContain("search_hybrid");
    expect(root).toContain("memory_read");
    expect(skills).toContain("skills_list");
    expect(web).toContain("web_search");
    expect(write).toContain("insert_text_at_cursor");
  });

  it("keeps tool_catalog_impl.rs below the current runtime split checkpoint", () => {
    expect(
      lineCount("src-tauri/src/ai_runtime/tool_catalog_impl.rs"),
    ).toBeLessThanOrEqual(260);
  });

  it("moves tool dispatch handlers behind domain modules", () => {
    const parent = read("src-tauri/src/ai_runtime/tool_dispatch_impl.rs");
    const markdown = read("src-tauri/src/ai_runtime/tool_dispatch/markdown.rs");
    const memory = read("src-tauri/src/ai_runtime/tool_dispatch/memory.rs");
    const note = read("src-tauri/src/ai_runtime/tool_dispatch/note.rs");
    const schedule = read("src-tauri/src/ai_runtime/tool_dispatch/schedule.rs");
    const search = read("src-tauri/src/ai_runtime/tool_dispatch/search.rs");
    const skills = read("src-tauri/src/ai_runtime/tool_dispatch/skills.rs");
    const web = read("src-tauri/src/ai_runtime/tool_dispatch/web.rs");

    expect(parent).toContain("tool_dispatch/markdown.rs");
    expect(parent).toContain("tool_dispatch/memory.rs");
    expect(parent).toContain("tool_dispatch/note.rs");
    expect(parent).toContain("tool_dispatch/schedule.rs");
    expect(parent).toContain("tool_dispatch/search.rs");
    expect(parent).toContain("tool_dispatch/skills.rs");
    expect(parent).toContain("tool_dispatch/web.rs");
    expect(parent).not.toMatch(/^async fn hybrid_search\(/m);
    expect(parent).not.toMatch(/^async fn read_note\(/m);
    expect(parent).not.toMatch(/^async fn web_search_tool\(/m);
    expect(parent).not.toMatch(/^async fn memory_read_tool\(/m);
    expect(parent).not.toMatch(/^async fn skills_install_tool\(/m);
    expect(markdown).toContain("markdown_write_patch_apply");
    expect(memory).toContain("memory_read_tool");
    expect(note).toContain("read_note");
    expect(schedule).toContain("scheduled_task_create_tool");
    expect(search).toContain("hybrid_search");
    expect(skills).toContain("skills_list_tool");
    expect(web).toContain("web_search_tool");
  });

  it("keeps tool_dispatch_impl.rs below the current runtime split checkpoint", () => {
    expect(
      lineCount("src-tauri/src/ai_runtime/tool_dispatch_impl.rs"),
    ).toBeLessThanOrEqual(360);
  });
});
