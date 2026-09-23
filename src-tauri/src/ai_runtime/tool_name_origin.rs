//! Q01／Q12 工具名来源链：关联「模型提议 → 网关解析 → 工具面版本 → 派发结果」。
//!
//! 未知工具名只以指纹进入记录，不回显原文。缺 C11 解析 hop 时不得归因于模型。

use std::collections::HashSet;

use serde_json::{json, Value};

use super::tool_catalog::catalog_find;

/// Where C11 extracted the tool name from the provider payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParsePath {
    OpenAiToolCalls,
    AnthropicToolUse,
    OpenAiResponses,
    MinimaxContent,
}

impl ParsePath {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiToolCalls => "openai_tool_calls",
            Self::AnthropicToolUse => "anthropic_tool_use",
            Self::OpenAiResponses => "openai_responses",
            Self::MinimaxContent => "minimax_content",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "openai_tool_calls" => Some(Self::OpenAiToolCalls),
            "anthropic_tool_use" => Some(Self::AnthropicToolUse),
            "openai_responses" => Some(Self::OpenAiResponses),
            "minimax_content" => Some(Self::MinimaxContent),
            _ => None,
        }
    }
}

/// Closed origin for an unknown or mismatched tool name. Never confirms model fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameOrigin {
    Unattributed,
    ModelGenerated,
    ProtocolParsed,
    NameMapped,
    PromptConvention,
}

impl NameOrigin {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unattributed => "unattributed",
            Self::ModelGenerated => "model-generated",
            Self::ProtocolParsed => "protocol-parsed",
            Self::NameMapped => "name-mapped",
            Self::PromptConvention => "prompt-convention",
        }
    }
}

/// One C11 parse hop. The name itself is never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedNameHop {
    pub fingerprint: String,
    pub catalog_known: bool,
    pub parse_path: ParsePath,
    pub rewritten: bool,
    pub model_turn: u32,
    pub tool_surface_version: String,
    pub declared_this_turn: bool,
}

/// Facts recorded at C17 for one proposal, before joining the C11 hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProposalNameFacts {
    pub catalog_known: bool,
    pub surface_known: bool,
    pub prompt_declared: bool,
    pub mapping_applied: bool,
}

/// Inputs for origin inference after C11 and C17 are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OriginInputs {
    pub parse_hop_present: bool,
    pub rewritten: bool,
    pub parse_path: Option<ParsePath>,
    pub facts: ProposalNameFacts,
}

/// C27 explanation of one proposal. Pending text is absent when the chain is a
/// complete catalog-surface accept path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProposalExplanation {
    pub origin: NameOrigin,
    pub display_tool: String,
    pub known_fact: String,
    pub pending_statement: Option<String>,
}

/// Stable fingerprint for a parsed tool name. Unknown names stay behind this hash.
pub(crate) fn name_fingerprint(name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(name.trim().as_bytes());
    format!("sha256:{}", &hex::encode(digest)[..16])
}

pub(crate) fn parse_path_for(_provider_name: &str, protocol_adapter: &str) -> ParsePath {
    match protocol_adapter {
        "anthropic_messages" => ParsePath::AnthropicToolUse,
        "openai_responses" => ParsePath::OpenAiResponses,
        _ => ParsePath::OpenAiToolCalls,
    }
}

pub(crate) fn infer_origin(input: OriginInputs) -> NameOrigin {
    if !input.parse_hop_present {
        return NameOrigin::Unattributed;
    }
    if input.rewritten || matches!(input.parse_path, Some(ParsePath::MinimaxContent)) {
        return NameOrigin::ProtocolParsed;
    }
    if input.facts.mapping_applied || (input.facts.surface_known && !input.facts.catalog_known) {
        return NameOrigin::NameMapped;
    }
    if input.facts.catalog_known && (!input.facts.surface_known || !input.facts.prompt_declared) {
        return NameOrigin::PromptConvention;
    }
    if !input.facts.catalog_known && !input.facts.surface_known {
        return NameOrigin::ModelGenerated;
    }
    NameOrigin::Unattributed
}

fn safe_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() || name.len() > 64 {
        return false;
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn display_tool(parsed_name: &str, facts: ProposalNameFacts) -> String {
    if facts.catalog_known {
        return catalog_find(parsed_name)
            .map(|entry| entry.name.to_string())
            .unwrap_or_else(|| "unknown".into());
    }
    if facts.surface_known && safe_ident(parsed_name) {
        return parsed_name.to_string();
    }
    "unknown".into()
}

/// C17 proposal payload. Raw unknown names are replaced by `unknown`.
pub(crate) fn proposal_payload(
    parsed_name: &str,
    surface_names: &[&str],
    declared_names: &[&str],
    mapping_applied: bool,
    dispatch_result: &str,
    model_turn: u32,
) -> Value {
    let facts = ProposalNameFacts {
        catalog_known: catalog_find(parsed_name).is_some(),
        surface_known: surface_names.contains(&parsed_name),
        prompt_declared: declared_names.contains(&parsed_name),
        mapping_applied,
    };
    let tool = display_tool(parsed_name, facts);
    json!({
        "event": "proposal",
        "tool": tool,
        "catalogKnown": facts.catalog_known,
        "surfaceKnown": facts.surface_known,
        "promptDeclared": facts.prompt_declared,
        "mappingApplied": facts.mapping_applied,
        "nameFingerprint": name_fingerprint(parsed_name),
        "reason": dispatch_result,
        "modelTurns": model_turn,
        "toolSurfaceVersion": super::boundary_events::tool_surface_version(surface_names.iter().copied()),
    })
}

/// C11 handshake extras: declared and parsed fingerprints, never raw unknown names.
#[cfg(test)]
pub(crate) fn handshake_payload(
    parse_path: ParsePath,
    declared_names: impl IntoIterator<Item = impl AsRef<str>>,
    parsed_names: impl IntoIterator<Item = impl AsRef<str>>,
    rewritten: bool,
) -> Value {
    let declared: Vec<String> = declared_names
        .into_iter()
        .map(|name| name_fingerprint(name.as_ref()))
        .collect();
    let parsed: Vec<Value> = parsed_names
        .into_iter()
        .map(|name| {
            let name = name.as_ref();
            json!({
                "fingerprint": name_fingerprint(name),
                "catalogKnown": catalog_find(name).is_some(),
                "parsePath": parse_path.as_str(),
                "rewritten": rewritten,
            })
        })
        .collect();
    json!({
        "declaredNameFingerprints": declared,
        "parsedToolNames": parsed,
    })
}

/// C11 hops for one turn: structured `tool_calls` stay on the adapter path;
/// MiniMax content extraction is a protocol rewrite.
pub(crate) fn handshake_payload_from_calls(
    protocol_adapter: &str,
    declared_names: impl IntoIterator<Item = impl AsRef<str>>,
    structured_names: impl IntoIterator<Item = impl AsRef<str>>,
    content_extracted_names: impl IntoIterator<Item = impl AsRef<str>>,
) -> Value {
    let declared: Vec<String> = declared_names
        .into_iter()
        .map(|name| name_fingerprint(name.as_ref()))
        .collect();
    let adapter_path = parse_path_for("", protocol_adapter);
    let mut parsed = Vec::new();
    for name in structured_names {
        let name = name.as_ref();
        parsed.push(json!({
            "fingerprint": name_fingerprint(name),
            "catalogKnown": catalog_find(name).is_some(),
            "parsePath": adapter_path.as_str(),
            "rewritten": false,
        }));
    }
    for name in content_extracted_names {
        let name = name.as_ref();
        parsed.push(json!({
            "fingerprint": name_fingerprint(name),
            "catalogKnown": catalog_find(name).is_some(),
            "parsePath": ParsePath::MinimaxContent.as_str(),
            "rewritten": true,
        }));
    }
    json!({
        "declaredNameFingerprints": declared,
        "parsedToolNames": parsed,
    })
}

pub(crate) fn hops_from_payload(payload: &Value) -> Vec<ParsedNameHop> {
    let declared: HashSet<&str> = payload
        .get("declaredNameFingerprints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    payload
        .get("parsedToolNames")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let fingerprint = item.get("fingerprint")?.as_str()?.to_string();
            Some(ParsedNameHop {
                catalog_known: item
                    .get("catalogKnown")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                parse_path: ParsePath::parse(item.get("parsePath")?.as_str()?)?,
                rewritten: item
                    .get("rewritten")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                declared_this_turn: declared.contains(fingerprint.as_str()),
                fingerprint,
                model_turn: 0,
                tool_surface_version: String::new(),
            })
        })
        .collect()
}

fn hop_for<'a>(fingerprint: &str, hops: &'a [ParsedNameHop]) -> Option<&'a ParsedNameHop> {
    hops.iter().find(|hop| hop.fingerprint == fingerprint)
}

fn payload_flag(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn payload_str<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// Explain a C17 proposal after joining C11 parse hops for the same fingerprint.
pub(crate) fn explain_proposal(payload: &Value, hops: &[ParsedNameHop]) -> ProposalExplanation {
    let mut facts = ProposalNameFacts {
        catalog_known: payload_flag(payload, "catalogKnown"),
        surface_known: payload_flag(payload, "surfaceKnown"),
        prompt_declared: payload_flag(payload, "promptDeclared"),
        mapping_applied: payload_flag(payload, "mappingApplied"),
    };
    let fingerprint = payload_str(payload, "nameFingerprint").unwrap_or("");
    let hop = hop_for(fingerprint, hops);
    if let Some(hop) = hop {
        facts.prompt_declared = hop.declared_this_turn;
    }
    let origin = infer_origin(OriginInputs {
        parse_hop_present: hop.is_some(),
        rewritten: hop.is_some_and(|hop| hop.rewritten),
        parse_path: hop.map(|hop| hop.parse_path),
        facts,
    });
    let display_tool = if facts.catalog_known {
        payload_str(payload, "tool")
            .filter(|name| *name != "unknown")
            .unwrap_or("unknown")
            .to_string()
    } else if facts.surface_known {
        payload_str(payload, "tool")
            .filter(|name| *name != "unknown" && safe_ident(name))
            .unwrap_or("未登记工具")
            .to_string()
    } else {
        "未登记工具".to_string()
    };
    let dispatch = payload_str(payload, "reason").unwrap_or("unknown");
    let parse_path = hop.map(|hop| hop.parse_path.as_str()).unwrap_or("missing");
    let surface_version = payload_str(payload, "toolSurfaceVersion").unwrap_or("missing");
    let chain = format!(
        "C11 解析({parse_path}) → C16 表面({surface_version}, {}) → C17 派发({dispatch})",
        if facts.surface_known {
            "包含"
        } else {
            "未包含"
        }
    );
    let known_fact = if origin == NameOrigin::Unattributed
        && hop.is_some()
        && facts.catalog_known
        && facts.surface_known
    {
        format!("工具提议 {display_tool} 来源链：{chain}")
    } else {
        format!(
            "工具提议 {display_tool} 来源链：{chain}（{}）",
            origin.as_str()
        )
    };
    let rejected = dispatch != "accepted";
    let pending_statement = match origin {
        NameOrigin::Unattributed if hop.is_none() => {
            Some("工具名来源链缺少网关解析 hop，不能归因于模型".to_string())
        }
        NameOrigin::Unattributed => None,
        NameOrigin::ModelGenerated => Some(format!(
            "未登记工具的来源为模型生成（疑似），不能证实为模型故障；{chain}"
        )),
        NameOrigin::ProtocolParsed => Some(format!(
            "未登记工具可能来自协议解析改写，不能归因于模型；{chain}"
        )),
        NameOrigin::NameMapped if rejected => {
            Some(format!("工具名经过名称映射，不能归因于模型；{chain}"))
        }
        NameOrigin::NameMapped => None,
        NameOrigin::PromptConvention => Some(format!(
            "目录内名称未出现在本次工具面，属提示约定或表面错位，不能归因于模型；{chain}"
        )),
    };
    ProposalExplanation {
        origin,
        display_tool,
        known_fact,
        pending_statement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(catalog_known: bool, surface_known: bool, mapping_applied: bool) -> ProposalNameFacts {
        ProposalNameFacts {
            catalog_known,
            surface_known,
            prompt_declared: catalog_known,
            mapping_applied,
        }
    }

    #[test]
    fn fingerprint_is_stable_and_does_not_embed_the_name() {
        let fingerprint = name_fingerprint("unknown_search");
        assert!(fingerprint.starts_with("sha256:"));
        assert_eq!(fingerprint.len(), "sha256:".len() + 16);
        assert!(!fingerprint.contains("unknown_search"));
        assert_eq!(fingerprint, name_fingerprint("unknown_search"));
        assert_ne!(fingerprint, name_fingerprint("web_search"));
    }

    #[test]
    fn missing_parse_hop_is_unattributed() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: false,
                rewritten: false,
                parse_path: None,
                facts: facts(false, false, false),
            }),
            NameOrigin::Unattributed
        );
    }

    #[test]
    fn unknown_name_with_parse_hop_is_model_generated() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: false,
                parse_path: None,
                facts: facts(false, false, false),
            }),
            NameOrigin::ModelGenerated
        );
    }

    #[test]
    fn rewritten_parse_is_protocol_parsed() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: true,
                parse_path: None,
                facts: facts(false, false, false),
            }),
            NameOrigin::ProtocolParsed
        );
    }

    #[test]
    fn mapped_surface_name_is_name_mapped() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: false,
                parse_path: None,
                facts: facts(false, true, true),
            }),
            NameOrigin::NameMapped
        );
    }

    #[test]
    fn catalog_name_absent_from_surface_is_prompt_convention() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: false,
                parse_path: None,
                facts: facts(true, false, false),
            }),
            NameOrigin::PromptConvention
        );
    }

    #[test]
    fn catalog_on_surface_but_not_declared_is_prompt_convention() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: false,
                parse_path: None,
                facts: ProposalNameFacts {
                    catalog_known: true,
                    surface_known: true,
                    prompt_declared: false,
                    mapping_applied: false,
                },
            }),
            NameOrigin::PromptConvention
        );
    }

    #[test]
    fn surface_external_name_without_mapping_flag_is_name_mapped() {
        assert_eq!(
            infer_origin(OriginInputs {
                parse_hop_present: true,
                rewritten: false,
                parse_path: None,
                facts: facts(false, true, false),
            }),
            NameOrigin::NameMapped
        );
    }

    #[test]
    fn proposal_payload_redacts_unknown_names() {
        let payload = proposal_payload(
            "unknown_search",
            &[],
            &["web_search"],
            false,
            "tool_not_in_run_surface",
            2,
        );
        let encoded = payload.to_string();
        assert!(!encoded.contains("unknown_search"));
        assert_eq!(payload["tool"], "unknown");
        assert_eq!(payload["catalogKnown"], false);
        assert_eq!(payload["surfaceKnown"], false);
        assert_eq!(payload["promptDeclared"], false);
        assert_eq!(
            payload["nameFingerprint"],
            name_fingerprint("unknown_search")
        );
    }

    #[test]
    fn proposal_payload_keeps_catalog_names() {
        let payload = proposal_payload(
            "web_search",
            &["web_search"],
            &["web_search"],
            false,
            "accepted",
            1,
        );
        assert_eq!(payload["tool"], "web_search");
        assert_eq!(payload["catalogKnown"], true);
        assert_eq!(payload["surfaceKnown"], true);
        assert_eq!(payload["promptDeclared"], true);
    }

    #[test]
    fn handshake_payload_redacts_unknown_names() {
        let payload = handshake_payload(
            ParsePath::OpenAiToolCalls,
            ["web_search"],
            ["unknown_search"],
            false,
        );
        let encoded = payload.to_string();
        assert!(!encoded.contains("unknown_search"));
        assert_eq!(
            payload["declaredNameFingerprints"][0],
            name_fingerprint("web_search")
        );
        assert_eq!(
            payload["parsedToolNames"][0]["fingerprint"],
            name_fingerprint("unknown_search")
        );
        assert_eq!(payload["parsedToolNames"][0]["catalogKnown"], false);
        assert_eq!(
            payload["parsedToolNames"][0]["parsePath"],
            "openai_tool_calls"
        );
    }

    #[test]
    fn explain_proposal_without_hop_does_not_blame_the_model() {
        let proposal = proposal_payload(
            "unknown_search",
            &[],
            &[],
            false,
            "tool_not_in_run_surface",
            1,
        );
        let explanation = explain_proposal(&proposal, &[]);
        assert_eq!(explanation.origin, NameOrigin::Unattributed);
        assert_eq!(explanation.display_tool, "未登记工具");
        assert!(explanation
            .pending_statement
            .as_deref()
            .is_some_and(|text| text.contains("不能归因于模型")));
        assert!(!explanation.known_fact.contains("unknown_search"));
    }

    #[test]
    fn explain_proposal_joins_c11_hop_for_model_generated() {
        let proposal = proposal_payload(
            "unknown_search",
            &[],
            &[],
            false,
            "tool_not_in_run_surface",
            1,
        );
        let hops = hops_from_payload(&handshake_payload(
            ParsePath::AnthropicToolUse,
            ["web_search"],
            ["unknown_search"],
            false,
        ));
        let explanation = explain_proposal(&proposal, &hops);
        assert_eq!(explanation.origin, NameOrigin::ModelGenerated);
        assert!(explanation
            .pending_statement
            .as_deref()
            .is_some_and(|text| text.contains("模型生成") && text.contains("不能证实")));
        assert!(explanation.known_fact.contains("anthropic_tool_use"));
    }

    #[test]
    fn parse_path_follows_adapter_not_vendor_name() {
        assert_eq!(
            parse_path_for("MiniMax", "openai_chat_completions"),
            ParsePath::OpenAiToolCalls
        );
        assert_eq!(
            parse_path_for("openai", "anthropic_messages"),
            ParsePath::AnthropicToolUse
        );
        assert_eq!(
            parse_path_for("openai", "openai_responses"),
            ParsePath::OpenAiResponses
        );
    }

    #[test]
    fn proposal_payload_records_model_turns_and_surface_version() {
        let payload = proposal_payload(
            "web_search",
            &["web_search"],
            &["web_search"],
            false,
            "accepted",
            2,
        );
        assert_eq!(payload["modelTurns"], 2);
        assert!(payload.get("modelTurn").is_none());
        assert_eq!(
            payload["toolSurfaceVersion"],
            crate::ai_runtime::boundary_events::tool_surface_version(["web_search"])
        );
        assert!(!payload["toolSurfaceVersion"]
            .as_str()
            .unwrap_or("")
            .is_empty());
    }

    #[test]
    fn explain_proposal_missing_hop_on_catalog_accept_is_a_gap() {
        let proposal = proposal_payload(
            "web_search",
            &["web_search"],
            &["web_search"],
            false,
            "accepted",
            1,
        );
        let explanation = explain_proposal(&proposal, &[]);
        assert_eq!(explanation.origin, NameOrigin::Unattributed);
        assert_eq!(explanation.display_tool, "web_search");
        assert!(
            explanation
                .pending_statement
                .as_deref()
                .is_some_and(
                    |text| text.contains("缺少网关解析 hop") && text.contains("不能归因于模型")
                ),
            "catalog accept without a C11 hop must not look complete: {:?}",
            explanation.pending_statement
        );
        assert!(explanation.known_fact.contains("C16 表面"));
        assert!(explanation.known_fact.contains(
            crate::ai_runtime::boundary_events::tool_surface_version(["web_search"]).as_str()
        ));
    }

    #[test]
    fn explain_proposal_minimax_content_path_is_protocol_parsed_even_if_not_flagged_rewritten() {
        let proposal = proposal_payload(
            "unknown_search",
            &[],
            &["web_search"],
            false,
            "tool_not_in_run_surface",
            1,
        );
        let hops = hops_from_payload(&handshake_payload(
            ParsePath::MinimaxContent,
            ["web_search"],
            ["unknown_search"],
            false,
        ));
        let explanation = explain_proposal(&proposal, &hops);
        assert_eq!(explanation.origin, NameOrigin::ProtocolParsed);
        assert!(explanation
            .pending_statement
            .as_deref()
            .is_some_and(|text| text.contains("协议解析") && text.contains("不能归因于模型")));
        assert!(!explanation
            .pending_statement
            .as_deref()
            .is_some_and(|text| text.contains("模型生成")));
    }

    #[test]
    fn explain_proposal_does_not_claim_missing_hop_when_hop_exists() {
        let proposal = proposal_payload(
            "weather_lookup",
            &["weather_lookup"],
            &["weather_lookup"],
            false,
            "accepted",
            1,
        );
        let hops = hops_from_payload(&handshake_payload(
            ParsePath::OpenAiToolCalls,
            ["weather_lookup"],
            ["weather_lookup"],
            false,
        ));
        let explanation = explain_proposal(&proposal, &hops);
        assert_eq!(explanation.origin, NameOrigin::NameMapped);
        assert!(explanation
            .pending_statement
            .as_deref()
            .is_none_or(|text| !text.contains("缺少网关解析 hop")));
    }

    #[test]
    fn handshake_payload_from_calls_marks_only_content_extraction_rewritten() {
        let payload = handshake_payload_from_calls(
            "openai_chat_completions",
            ["web_search"],
            ["web_search"],
            ["unknown_search"],
        );
        let hops = hops_from_payload(&payload);
        assert_eq!(hops.len(), 2);
        let structured = hops
            .iter()
            .find(|hop| hop.fingerprint == name_fingerprint("web_search"))
            .expect("structured hop");
        assert_eq!(structured.parse_path, ParsePath::OpenAiToolCalls);
        assert!(!structured.rewritten);
        let extracted = hops
            .iter()
            .find(|hop| hop.fingerprint == name_fingerprint("unknown_search"))
            .expect("content hop");
        assert_eq!(extracted.parse_path, ParsePath::MinimaxContent);
        assert!(extracted.rewritten);
        assert!(!payload.to_string().contains("unknown_search"));
    }
}
