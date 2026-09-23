//! Main-dialogue search-event recognition.
//!
//! Isolated native search stays `stream:false` and does not use this module.
//! When a vendor leaks `web_search_call` / `server_tool_use` / citations onto
//! the main SSE, fold them into a `MainStreamLeak` observation. Never turn
//! those events into executable client `ToolCall`s.

use crate::ai_runtime::native_search_subrequest::{
    merge_retrieval_observation, observation_from_main_stream_json, RetrievalObservation,
};

pub(super) fn note_stream_search_json(
    slot: &mut Option<RetrievalObservation>,
    json: &serde_json::Value,
) {
    if let Some(next) = observation_from_main_stream_json(json) {
        merge_retrieval_observation(slot, next);
    }
}
