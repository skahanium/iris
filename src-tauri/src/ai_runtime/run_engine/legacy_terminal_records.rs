//! Read-side adapter for persisted terminal answers that predate an explicit
//! terminal type.
//!
//! Old Runs stored only the assistant body, so the only surviving record of
//! "the Host wrote this, not the Provider" is the body's own opening text. This
//! adapter owns both halves of that legacy disclosure shape: the exact opening
//! constants its producers wrote, and the predicate that recognises them.
//!
//! Two rules keep it from becoming a bypass:
//!
//! 1. It reads persisted records only. The live tool loop publishes an
//!    [`crate::ai_runtime::agent_tool_loop::AgentTerminalType`], and no code
//!    path that validates a *new* answer consults these predicates.
//! 2. A body that merely starts with the same words still has to match the
//!    whole legacy disclosure — the opening phrase plus the verification
//!    statement the producer always wrote — so ordinary prose cannot earn the
//!    exemption by prefix alone.

/// The opening of a legacy limitation published when the Run held no usable
/// verifiable body.
pub(crate) const EVIDENCE_LIMITED_RESPONSE_PREFIX: &str = "本轮未取得足够的可核验来源正文";

/// The opening of a legacy limitation published when the Run already held
/// verifiable material but could not finish attribution.
pub(crate) const EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX: &str = "本轮已取得可核验资料";

/// The verification statements the legacy "no verifiable body" producer always
/// wrote, one of which every such record closes with.
///
/// The producer emitted the canonical Host body when no Web failure code was
/// known, the fixed lead-in `无法据此确认当前情况。` before a reason-specific
/// `web_fetch` suffix, that same lead-in with a full-width colon when it
/// introduced the unverified-lead list, or one of the two fixed Web-failure
/// verdicts. Requiring one of them is what makes the disclosure complete rather
/// than a bare opening phrase.
///
/// One legacy producer branch has no fixed closing verdict: the
/// `现有材料不足以支持答复中的事实…` copy is emitted with a generic
/// `无法确认` tail that ordinary prose can also produce. A persisted record of
/// that one body is therefore *not* recognised as Host-authored. That is the
/// deliberate conservative direction: an unrecognised record is simply treated
/// as model output and validated, while a loose match would hand arbitrary
/// prose a validation exemption.
#[cfg(test)]
const LEGACY_NO_BODY_VERDICTS: [&str; 5] = [
    // The canonical Host body. It is the exact published text, not a fragment
    // the model could reproduce on its own.
    crate::ai_runtime::agent_tool_loop::EVIDENCE_LIMITED_RESPONSE,
    // The producer's own fixed lead-in before a `web_fetch` reason suffix.
    "无法据此确认当前情况。",
    // The same lead-in when it introduces the unverified-lead list.
    "无法据此确认当前情况：",
    // The producer's two fixed Web-failure verdicts.
    "未取得可用结果。",
    "未取得外部资料。",
];

/// Whether a persisted terminal body was authored by the Host rather than the
/// Provider.
///
/// A record qualifies only when it carries the full legacy disclosure: the
/// opening phrase plus the verification verdict the producer always wrote. A
/// model answer that merely opens with the same words therefore stays model
/// output and is still validated.
#[cfg(test)]
pub(crate) fn legacy_record_is_host_authored(content: &str) -> bool {
    let trimmed = content.trim_start();
    if trimmed.starts_with(EVIDENCE_LIMITED_RESPONSE_PREFIX) {
        return LEGACY_NO_BODY_VERDICTS
            .iter()
            .any(|verdict| trimmed.contains(verdict));
    }
    trimmed.starts_with(EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX)
        && trimmed.contains("但未能完成最终来源关联")
}
