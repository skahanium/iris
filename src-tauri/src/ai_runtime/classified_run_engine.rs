//! CEF-only direct execution for classified Agent Runs.
//!
//! The executor deliberately never persists streamed tokens through the common
//! event channel. Classified content remains in the CEF conversation; the UI
//! receives only safe lifecycle events and reloads the final message from CEF.

use std::path::Path;

use crate::ai_runtime::classified_session::{
    classified_run_complete, classified_run_deny_before_dispatch, classified_run_fail,
    classified_run_mark_preparing, classified_run_mark_running, classified_run_policy_request,
    classified_run_user_message,
};
use crate::ai_runtime::run_contract::{AssistantSessionRef, SafeRunErrorCode};
use crate::ai_runtime::run_engine::{RunEventSink, StreamingDirectAnswerProvider};
use crate::error::{AppError, AppResult};

struct ClassifiedStreamObserver;

impl crate::ai_runtime::model_gateway::StreamEventObserver for ClassifiedStreamObserver {
    fn observe(
        &mut self,
        _event: &crate::ai_runtime::model_gateway::StreamEvent,
        _token_index: u32,
    ) -> AppResult<()> {
        // Stream payload may contain classified text. It is intentionally not
        // forwarded to the general-purpose Tauri event bus or normal storage.
        Ok(())
    }
}

/// Execute one accepted classified direct Run without touching normal Run storage.
pub(crate) async fn execute_classified_direct_streaming_with_sink(
    vault: &Path,
    session: &AssistantSessionRef,
    run_id: &str,
    provider: &impl StreamingDirectAnswerProvider,
    sink: &impl RunEventSink,
) -> AppResult<()> {
    let request = classified_run_policy_request(vault, session, run_id)?;
    let engine = crate::ai_runtime::classified_document_policy_repository::load_classified_policy_decision_engine(vault)?;
    let decision = engine.evaluate_run(request);
    if decision.denial_code.is_some() {
        for event in classified_run_deny_before_dispatch(vault, session, run_id, &decision)? {
            sink.emit(&event)?;
        }
        return Err(AppError::msg("agent_run_permission_denied"));
    }
    let preparing = classified_run_mark_preparing(vault, session, run_id)?;
    sink.emit(&preparing)?;
    let running = classified_run_mark_running(vault, session, run_id, 1)?;
    sink.emit(&running)?;
    let message = classified_run_user_message(vault, session, run_id)?;
    let messages = [crate::ai_runtime::LlmMessage {
        role: crate::ai_runtime::MessageRole::User,
        content: crate::ai_types::MessageContent::Text(message),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }];
    let response = {
        let mut observer = ClassifiedStreamObserver;
        provider
            .answer_streaming(run_id, &messages, &mut observer)
            .await
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            let code = classify_classified_provider_failure(&error);
            if let Some(failed) = classified_run_fail(vault, session, run_id, 2, code)? {
                sink.emit(&failed)?;
            }
            return Err(AppError::msg(code.as_str()));
        }
    };
    if !response.tool_calls.is_empty() || response.content.as_deref().is_none_or(str::is_empty) {
        if let Some(failed) =
            classified_run_fail(vault, session, run_id, 2, SafeRunErrorCode::InvalidRequest)?
        {
            sink.emit(&failed)?;
        }
        return Err(AppError::msg("agent_run_direct_response_invalid"));
    }
    let completed = classified_run_complete(
        vault,
        session,
        run_id,
        2,
        response.content.expect("validated non-empty content"),
    )?;
    sink.emit(&completed)
}

fn classify_classified_provider_failure(error: &AppError) -> SafeRunErrorCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("first_response_timeout")
        || message.contains("stream_idle_timeout")
        || message.contains("timed out")
        || message.contains("timeout")
        || message.contains("deadline")
    {
        SafeRunErrorCode::ProviderTimeout
    } else {
        SafeRunErrorCode::ProviderUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    use uuid::Uuid;

    use crate::ai_runtime::classified_session::{
        classified_ai_thread_load, classified_run_accept, ClassifiedRunAcceptInput,
    };
    use crate::ai_runtime::run_contract::{AssistantRunEvent, SecurityDomain};
    use crate::crypto::vault_key::{VAULT_KEY, VAULT_KEY_TEST_LOCK};

    fn write_document_deny_policy(vault: &Path) {
        let plaintext = serde_json::json!({
            "version": 1,
            "rules": [{
                "scope_kind": "document",
                "scope_path": "restricted.md",
                "capability": "send_to_model",
                "decision": "deny"
            }]
        });
        let vault_key = VAULT_KEY
            .get()
            .expect("vault key initialized")
            .read()
            .expect("vault key read lock");
        let ciphertext = crate::crypto::classified_io::encrypt_cef(
            &serde_json::to_vec(&plaintext).expect("policy fixture json"),
            vault_key.key().expect("vault key available"),
        )
        .expect("encrypt policy fixture");
        let policy_dir = vault.join(".classified").join(".iris-ai");
        fs::create_dir_all(&policy_dir).expect("create policy directory");
        fs::write(policy_dir.join("document-policies.cef"), ciphertext)
            .expect("write policy fixture");
    }
    fn classified_envelope() -> serde_json::Value {
        serde_json::to_value(crate::ai_runtime::run_contract::ExecutionEnvelope {
            effect: crate::ai_runtime::run_contract::Effect::Answer,
            context: crate::ai_runtime::run_contract::ContextMode::ExplicitReferences,
            freshness: crate::ai_runtime::run_contract::Freshness::Offline,
            web_reason: crate::ai_runtime::run_contract::WebDecisionReason::SecurityDomainOffline,
            effort: crate::ai_runtime::run_contract::Effort::Direct,
            security_domain: SecurityDomain::Classified,
            risk: crate::ai_runtime::run_contract::RiskClass::ReadOnly,
            modalities: vec![crate::ai_runtime::run_contract::Modality::Text],
            material_needs: Vec::new(),
            required_capabilities: vec![crate::ai_runtime::run_contract::CapabilityId::new(
                "model.text",
            )],
            explicit_constraints: Vec::new(),
        })
        .unwrap()
    }
    struct FixedProvider;

    impl StreamingDirectAnswerProvider for FixedProvider {
        fn answer_streaming<'a>(
            &'a self,
            _run_id: &'a str,
            _messages: &'a [crate::ai_runtime::LlmMessage],
            _observer: &'a mut dyn crate::ai_runtime::model_gateway::StreamEventObserver,
        ) -> Pin<
            Box<
                dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async {
                Ok(crate::ai_runtime::model_gateway::GatewayResponse {
                    content: Some("confidential answer".into()),
                    tool_calls: Vec::new(),
                    usage: crate::ai_types::TokenUsage::default(),
                    finish_reason: "stop".into(),
                    reasoning_content: None,
                })
            })
        }
    }
    struct CountingProvider(AtomicU32);

    struct FailingProvider;

    impl StreamingDirectAnswerProvider for FailingProvider {
        fn answer_streaming<'a>(
            &'a self,
            _run_id: &'a str,
            _messages: &'a [crate::ai_runtime::LlmMessage],
            _observer: &'a mut dyn crate::ai_runtime::model_gateway::StreamEventObserver,
        ) -> Pin<
            Box<
                dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Err(AppError::msg("llm_stream_first_response_timeout")) })
        }
    }

    impl StreamingDirectAnswerProvider for CountingProvider {
        fn answer_streaming<'a>(
            &'a self,
            _run_id: &'a str,
            _messages: &'a [crate::ai_runtime::LlmMessage],
            _observer: &'a mut dyn crate::ai_runtime::model_gateway::StreamEventObserver,
        ) -> Pin<
            Box<
                dyn Future<Output = AppResult<crate::ai_runtime::model_gateway::GatewayResponse>>
                    + Send
                    + 'a,
            >,
        > {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(AppError::msg("provider must not be called")) })
        }
    }
    struct RecordingSink {
        events: Mutex<Vec<serde_json::Value>>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl RunEventSink for RecordingSink {
        fn emit(&self, event: &AssistantRunEvent) -> AppResult<()> {
            self.events
                .lock()
                .unwrap()
                .push(serde_json::to_value(event)?);
            Ok(())
        }
    }

    #[test]
    fn classified_document_deny_prevents_provider_dispatch_and_persists_safe_event() {
        let _test_lock = VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        let mut key = VAULT_KEY
            .get()
            .expect("vault key initialized")
            .write()
            .expect("vault key write lock");
        key.set_test_key([19; 32]);
        drop(key);
        let vault =
            std::env::temp_dir().join(format!("iris-classified-policy-run-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        write_document_deny_policy(&vault);
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-policy-deny".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000050".into(),
                turn_id: "019f0871-0000-7000-8000-000000000051".into(),
                message: "confidential question".into(),
                content_parts: None,
                explicit_references: vec![serde_json::json!({ "filePath": "restricted.md" })],
                explicit_action: None,
                envelope: classified_envelope(),
                effect: "answer".into(),
            },
        )
        .unwrap();
        let provider = CountingProvider(AtomicU32::new(0));
        let sink = RecordingSink::new();

        let error = tauri::async_runtime::block_on(execute_classified_direct_streaming_with_sink(
            &vault,
            &accepted.session,
            &accepted.run_id,
            &provider,
            &sink,
        ))
        .expect_err("CEF policy deny must stop provider dispatch");

        assert_eq!(error.to_string(), "agent_run_permission_denied");
        assert_eq!(provider.0.load(Ordering::SeqCst), 0);
        let stored = classified_ai_thread_load(&vault, accepted.session.session_key).unwrap();
        assert_eq!(stored.runs[0].status, "failed");
        assert!(stored
            .events
            .iter()
            .any(|event| event.event_type == "permission_denied"));
        fs::remove_dir_all(vault).unwrap();
    }
    #[test]
    fn classified_direct_execution_persists_only_cef_facts() {
        let _test_lock = VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        let mut key = VAULT_KEY
            .get()
            .expect("vault key initialized")
            .write()
            .expect("vault key write lock");
        key.set_test_key([7; 32]);
        drop(key);
        let vault = std::env::temp_dir().join(format!("iris-classified-engine-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-engine-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000040".into(),
                turn_id: "019f0871-0000-7000-8000-000000000041".into(),
                message: "confidential question".into(),
                content_parts: None,
                explicit_references: Vec::new(),
                explicit_action: None,
                envelope: classified_envelope(),
                effect: "answer".into(),
            },
        )
        .unwrap();
        assert_eq!(accepted.session.domain, SecurityDomain::Classified);

        let sink = RecordingSink::new();
        tauri::async_runtime::block_on(execute_classified_direct_streaming_with_sink(
            &vault,
            &accepted.session,
            &accepted.run_id,
            &FixedProvider,
            &sink,
        ))
        .expect("classified execution");

        let stored = classified_ai_thread_load(&vault, accepted.session.session_key).unwrap();
        assert_eq!(stored.messages.len(), 2);
        assert_eq!(stored.messages[1].content, "confidential answer");
        assert_eq!(stored.runs[0].status, "completed");
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|event| {
            event["type"] != "content_delta" && !event.to_string().contains("confidential answer")
        }));
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn classified_streaming_timeout_persists_the_distinct_safe_failure() {
        let _test_lock = VAULT_KEY_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::crypto::vault_key::init_vault_key();
        let mut key = VAULT_KEY
            .get()
            .expect("vault key initialized")
            .write()
            .expect("vault key write lock");
        key.set_test_key([8; 32]);
        drop(key);
        let vault = std::env::temp_dir().join(format!("iris-classified-engine-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();
        let accepted = classified_run_accept(
            &vault,
            ClassifiedRunAcceptInput {
                client_request_id: "classified-timeout-request".into(),
                session_key: None,
                run_id: "019f0871-0000-7000-8000-000000000042".into(),
                turn_id: "019f0871-0000-7000-8000-000000000043".into(),
                message: "confidential question".into(),
                content_parts: None,
                explicit_references: Vec::new(),
                explicit_action: None,
                envelope: classified_envelope(),
                effect: "answer".into(),
            },
        )
        .unwrap();
        let sink = RecordingSink::new();

        let error = tauri::async_runtime::block_on(execute_classified_direct_streaming_with_sink(
            &vault,
            &accepted.session,
            &accepted.run_id,
            &FailingProvider,
            &sink,
        ))
        .expect_err("timeout must terminalize the classified Run");

        assert_eq!(error.to_string(), "agent_run_provider_timeout");
        let stored = classified_ai_thread_load(&vault, accepted.session.session_key).unwrap();
        assert_eq!(stored.runs[0].status, "failed");
        let failed = stored.events.last().expect("failed event");
        assert_eq!(failed.payload["code"], "agent_run_provider_timeout");
        fs::remove_dir_all(vault).unwrap();
    }
}
