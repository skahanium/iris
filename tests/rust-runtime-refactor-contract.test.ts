import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Rust Agent Run runtime module contract", () => {
  it("keeps model gateway transport concerns in dedicated modules without retired prompt paths", () => {
    const parent = read("src-tauri/src/ai_runtime/model_gateway_impl.rs");
    const abort = read("src-tauri/src/ai_runtime/model_gateway/abort.rs");
    const body = read("src-tauri/src/ai_runtime/model_gateway/body.rs");
    const httpBackend = read(
      "src-tauri/src/ai_runtime/model_gateway/http_backend.rs",
    );
    const messages = read("src-tauri/src/ai_runtime/model_gateway/messages.rs");
    const streaming = read(
      "src-tauri/src/ai_runtime/model_gateway/streaming.rs",
    );
    const usage = read("src-tauri/src/ai_runtime/model_gateway/usage.rs");

    for (const module of [
      "abort",
      "body",
      "http_backend",
      "messages",
      "streaming",
      "usage",
    ]) {
      expect(parent).toContain(`model_gateway/${module}.rs`);
    }
    expect(parent).not.toContain("model_gateway/prompts.rs");
    expect(parent).not.toMatch(/^pub fn build_drafting_prompt\(/m);
    expect(parent).not.toMatch(/^pub fn build_citation_prompt\(/m);
    expect(abort).toContain("request_abort");
    expect(body).toContain("GatewayRequest");
    expect(httpBackend).toContain("HttpLlmBackend");
    expect(messages).toContain("messages_for_api");
    expect(streaming).toContain("send_streaming_request");
    expect(usage).toContain("parse_usage");
  });

  it("keeps retrieval broker algorithms split by source and ranking responsibility", () => {
    const parent = read("src-tauri/src/ai_runtime/retrieval_broker_impl.rs");
    const fts = read("src-tauri/src/ai_runtime/retrieval_broker/fts.rs");
    const graph = read("src-tauri/src/ai_runtime/retrieval_broker/graph.rs");
    const vector = read("src-tauri/src/ai_runtime/retrieval_broker/vector.rs");
    const exact = read("src-tauri/src/ai_runtime/retrieval_broker/exact.rs");
    const template = read(
      "src-tauri/src/ai_runtime/retrieval_broker/template.rs",
    );
    const rank = read("src-tauri/src/ai_runtime/retrieval_broker/rank.rs");

    for (const module of [
      "fts",
      "graph",
      "vector",
      "exact",
      "template",
      "rank",
    ]) {
      expect(parent).toContain(`retrieval_broker/${module}.rs`);
    }
    expect(fts).toContain("search_fts");
    expect(graph).toContain("search_graph_neighbors");
    expect(vector).toContain("search_vector_chunks");
    expect(vector).toContain("c.content");
    expect(vector).not.toContain("c.text");
    expect(exact).toContain("search_exact_regulation");
    expect(template).toContain("search_template");
    expect(rank).toContain("fuse_and_rank");
  });

  it("activates skills from task facts instead of retired scenes", () => {
    const parent = read("src-tauri/src/ai_runtime/skills_impl.rs");
    const activation = read("src-tauri/src/ai_runtime/skills/activation.rs");
    const frontmatter = read("src-tauri/src/ai_runtime/skills/frontmatter.rs");
    const prompt = read("src-tauri/src/ai_runtime/skills/prompt.rs");
    const scan = read("src-tauri/src/ai_runtime/skills/scan.rs");

    for (const module of [
      "activation",
      "frontmatter",
      "prompt",
      "scan",
      "validation",
    ]) {
      expect(parent).toContain(`skills/${module}.rs`);
    }
    expect(activation).toContain("rank_skills_for_task");
    expect(activation).toContain("active_skills_for_task_prompt");
    expect(activation).not.toContain("rank_skills_for_scene");
    expect(activation).not.toContain("active_skills_for_prompt");
    expect(frontmatter).toContain("parse_frontmatter");
    expect(prompt).toContain("inject_into_prompt");
    expect(scan).toContain("scan_all_metadata");
  });

  it("keeps tool catalog, dispatch, permission, and audit under Run identity", () => {
    const catalog = read("src-tauri/src/ai_runtime/tool_catalog_impl.rs");
    const dispatch = read("src-tauri/src/ai_runtime/tool_dispatch_impl.rs");
    const loop = read("src-tauri/src/ai_runtime/run_tool_loop.rs");
    const pipeline = read(
      "src-tauri/src/ai_runtime/tool_execution_pipeline.rs",
    );
    const audit = read("src-tauri/src/ai_runtime/tool_audit.rs");

    expect(catalog).toContain("tool_catalog/groups.rs");
    expect(catalog).toContain("tool_catalog/web.rs");
    expect(dispatch).toContain("tool_dispatch/search.rs");
    expect(dispatch).toContain("tool_dispatch/web.rs");
    expect(loop).toContain("ToolExecutionGate");
    expect(loop).toContain("accepted.run_id");
    expect(pipeline).toContain("pub run_id: &'a str");
    expect(audit).toContain("run_id");
    expect(audit).toContain("run_step");
    expect(audit).not.toContain("pub request_id:");
    expect(audit).not.toContain("pub harness_round:");
  });

  it("keeps classified search filtering as valid SQL", () => {
    const engine = read("src-tauri/src/embedding/engine.rs");

    expect(engine).toContain("f.path <> '.classified'");
    expect(engine).toContain("f.path NOT LIKE '.classified/%'");
    expect(engine).not.toContain("''.classified/%''");
  });
});
