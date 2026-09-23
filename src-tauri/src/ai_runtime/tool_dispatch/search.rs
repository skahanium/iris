use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) async fn hybrid_search(
    state: &AppState,
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing query"))?;
    let limit = (args["limit"].as_u64().unwrap_or(10) as usize).clamp(1, 8);
    let layers = match tool_name {
        "search_keyword" => RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        "search_semantic" => RetrievalLayers {
            fts: false,
            vector: true,
            graph: false,
            exact: false,
            template: false,
        },
        _ => RetrievalLayers {
            fts: true,
            vector: true,
            graph: ctx.note_path.is_some(),
            exact: false,
            template: false,
        },
    };
    let (packets, status) = state.db.with_read_conn(|conn| {
        let request = RetrievalRequest {
            query: query.to_string(),
            max_results: limit,
            layers,
            note_context: ctx.note_path.map(|s| s.to_string()),
            file_id_context: ctx.file_id,
            scope: ctx.retrieval_scope.clone(),
            runtime_documents: ctx.runtime_documents.to_vec(),
            corpus_config: None,
        };
        let outcome =
            crate::ai_runtime::retrieval_broker::hybrid_retrieve_with_diagnostics(conn, &request)?;
        let mut packets = outcome.packets;
        packets.retain(|packet| {
            packet.source_path.as_deref().is_none_or(|path| {
                ctx.ensure_document_capability(
                    path,
                    crate::ai_runtime::policy_decision_engine::DocumentCapability::Read,
                )
                .is_ok()
            })
        });
        Ok((packets, retrieval_status(&outcome.diagnostics)))
    })?;
    Ok(serde_json::json!({
        "results": packets,
        "count": packets.len(),
        "retrievalStatus": status,
    }))
}

/// A bounded, provider-content-free summary of which retrieval layers ran.
///
/// Without this the model saw only "results + count" and could not tell an empty
/// vault apart from a broken index, so it could not choose a different strategy.
/// Only the layer name and its typed status are published — never the free-text
/// diagnostic message, model identifiers, or any path.
fn retrieval_status(
    diagnostics: &[crate::ai_runtime::retrieval_broker::RetrievalLayerDiagnostic],
) -> serde_json::Value {
    const MAX_LAYERS: usize = 8;
    let layers: Vec<serde_json::Value> = diagnostics
        .iter()
        .take(MAX_LAYERS)
        .map(|diagnostic| {
            serde_json::json!({
                "layer": diagnostic.layer,
                "status": diagnostic.status,
            })
        })
        .collect();
    let degraded = diagnostics.iter().any(|diagnostic| {
        diagnostic.status != crate::ai_runtime::retrieval_broker::RetrievalLayerStatus::Ok
            && diagnostic.status != crate::ai_runtime::retrieval_broker::RetrievalLayerStatus::Empty
    });
    serde_json::json!({ "degraded": degraded, "layers": layers })
}

pub(super) async fn regulation_lookup(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let regulation_name = args["regulation_name"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing regulation_name"))?;
    let article = args["article"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing article"))?;
    let article = crate::knowledge::regulations::normalize_regulation_label(article, '条')
        .ok_or_else(|| AppError::msg("invalid article"))?;
    let paragraph = args["paragraph"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let paragraph = paragraph
        .map(|value| {
            crate::knowledge::regulations::normalize_regulation_label(value, '款')
                .ok_or_else(|| AppError::msg("invalid paragraph"))
        })
        .transpose()?;
    let regulation_name = regulation_name
        .trim()
        .trim_start_matches('《')
        .trim_end_matches('》');
    let query = match paragraph.as_deref() {
        Some(paragraph) => format!("《{regulation_name}》{article}{paragraph}"),
        None => format!("《{regulation_name}》{article}"),
    };
    let packets = state.db.with_read_conn(|conn| {
        let request = RetrievalRequest {
            query,
            max_results: 3,
            layers: RetrievalLayers {
                fts: false,
                vector: false,
                graph: false,
                exact: true,
                template: false,
            },
            note_context: None,
            file_id_context: None,
            scope: ctx.retrieval_scope.clone(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        let mut packets = crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)?;
        packets.retain(|packet| {
            packet.source_path.as_deref().is_none_or(|path| {
                ctx.ensure_document_capability(
                    path,
                    crate::ai_runtime::policy_decision_engine::DocumentCapability::Read,
                )
                .is_ok()
            })
        });
        Ok(packets)
    })?;
    Ok(serde_json::json!({
        "regulation": packets.first(),
        "found": !packets.is_empty(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::retrieval_broker::{RetrievalLayerDiagnostic, RetrievalLayerStatus};

    #[tokio::test]
    async fn review_regression_ef_index_to_handler_normalizes_paragraph_and_keeps_policy() {
        use crate::ai_runtime::policy_decision_engine::{
            CapabilityDecision, DocumentCapability, DocumentPolicy, PolicyDecisionEngine,
        };
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(dir.path().join("data")).unwrap();
        state.db.with_conn(|conn| {
            conn.execute("INSERT INTO files(path,title,content_hash,created_at,updated_at) VALUES('law.md','条例','hash','now','now')", [])?;
            let file_id = conn.last_insert_rowid();
            let parsed = crate::knowledge::regulations::parse_regulation_structure("law.md", "《条例》\n第六条 内容\n一款 第一段\n二款 第二段");
            assert_eq!(parsed.clauses.len(), 2);
            crate::knowledge::regulations::index_regulation_clauses(conn, file_id, &parsed.clauses)?;
            Ok(())
        }).unwrap();
        let scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
        let policy = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        let args =
            serde_json::json!({"regulation_name":"条例","article":"第六条","paragraph":"第一款"});
        let mut ctx = ToolDispatchContext::for_tests(&scope);
        ctx.document_policy = Some(&policy);
        assert_eq!(
            regulation_lookup(&state, &args, &ctx).await.unwrap()["found"],
            true
        );
        let mut denied_policy = PolicyDecisionEngine::new(DocumentPolicy::allow_all());
        denied_policy.set_document_policy(
            "law.md",
            DocumentPolicy::from_rules([(DocumentCapability::Read, CapabilityDecision::Deny)]),
        );
        ctx.document_policy = Some(&denied_policy);
        assert_eq!(
            regulation_lookup(&state, &args, &ctx).await.unwrap()["found"],
            false
        );
    }

    fn diagnostic(
        layer: &str,
        status: RetrievalLayerStatus,
        message: &str,
    ) -> RetrievalLayerDiagnostic {
        RetrievalLayerDiagnostic {
            layer: layer.into(),
            status,
            message: Some(message.into()),
            backend: Some("sqlite-vec".into()),
            model_id: Some("internal-model-id".into()),
            generation_id: Some("generation-7".into()),
        }
    }

    #[test]
    fn retrieval_status_publishes_layer_states_without_internal_detail() {
        let status = retrieval_status(&[
            diagnostic("fts", RetrievalLayerStatus::Ok, "12 hits"),
            diagnostic(
                "vector",
                RetrievalLayerStatus::IndexNotReady,
                "model missing at /Users/x",
            ),
        ]);
        assert_eq!(status["degraded"], serde_json::json!(true));
        let layers = status["layers"].as_array().expect("layers");
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0]["layer"], serde_json::json!("fts"));
        assert_eq!(layers[0]["status"], serde_json::json!("ok"));
        assert_eq!(layers[1]["status"], serde_json::json!("index_not_ready"));

        // No free-text diagnostics, identifiers, or paths reach the model.
        let rendered = status.to_string();
        for leaked in [
            "/Users/x",
            "internal-model-id",
            "generation-7",
            "model missing",
        ] {
            assert!(!rendered.contains(leaked), "leaked {leaked}: {rendered}");
        }
    }

    #[test]
    fn an_empty_layer_is_not_reported_as_degraded() {
        let status = retrieval_status(&[
            diagnostic("fts", RetrievalLayerStatus::Empty, "0 hits"),
            diagnostic("vector", RetrievalLayerStatus::Ok, "3 hits"),
        ]);
        assert_eq!(status["degraded"], serde_json::json!(false));
    }

    #[test]
    fn the_status_block_is_bounded() {
        let many: Vec<_> = (0..20)
            .map(|index| diagnostic(&format!("layer-{index}"), RetrievalLayerStatus::Ok, "x"))
            .collect();
        let status = retrieval_status(&many);
        assert_eq!(status["layers"].as_array().expect("layers").len(), 8);
    }
}
