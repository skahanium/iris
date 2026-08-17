//! Streaming parser for MiniMax content-embedded tool calls.
//!
//! MiniMax M3 and some compatible gateways can emit tool calls inside the
//! text/content channel using control markers instead of the standard
//! `delta.tool_calls` field. This module keeps those JSON fragments out of the
//! visible answer and converts complete fragments into `ToolCall` values.
//!
//! The observed delimiter `]<|minimax|>[` acts as both the close of the
//! previous tool-call block and the open of the next one. The parser therefore
//! stays in tool mode after closing a block and only leaves tool mode when the
//! next non-whitespace character is clearly not the start of a JSON object or
//! array.

use crate::ai_types::ToolCall;

const MINIMAX_TOOL_DELIMITERS: &[&str] = &["]<|minimax|>[", "<|minimax|>"];

/// Parses MiniMax content deltas into visible text and completed tool calls.
#[derive(Debug, Default)]
pub struct MinimaxContentToolCallParser {
    pending: String,
    in_tool_call: bool,
    tool_json: String,
    tool_calls: Vec<ToolCall>,
    next_call_index: usize,
}

impl MinimaxContentToolCallParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one content delta. Returns text that is safe to show and any
    /// tool calls whose JSON blocks have been fully closed.
    pub fn push(&mut self, delta: &str) -> (String, Vec<ToolCall>) {
        self.pending.push_str(delta);
        let mut visible = String::new();
        let mut completed = Vec::new();

        loop {
            let Some((position, delimiter)) = self.earliest_delimiter(&self.pending) else {
                self.flush_non_tool_visible(&mut visible);
                break;
            };

            if self.in_tool_call {
                self.tool_json.push_str(&self.pending[..position]);
                self.pending.drain(..position + delimiter.len());
                if let Some(call) = self.finish_tool_call() {
                    completed.push(call);
                }
                if delimiter == "<|minimax|>" {
                    // Legacy marker uses a separate closing marker.
                    self.in_tool_call = false;
                }
                // The observed bracket delimiter closes the current block and
                // opens the next one, so stay in tool mode.
            } else {
                visible.push_str(&self.pending[..position]);
                self.pending.drain(..position + delimiter.len());
                self.in_tool_call = true;
            }
        }

        (visible, completed)
    }

    /// Flushes any remaining visible text and returns accumulated tool calls.
    /// An unclosed final tool-call JSON is accepted only when it is the last
    /// block; otherwise it is discarded so it cannot leak.
    pub fn finish(mut self) -> (String, Vec<ToolCall>) {
        if self.in_tool_call && !self.tool_json.trim().is_empty() {
            if let Some(call) = self.finish_tool_call() {
                self.tool_calls.push(call);
            }
        } else {
            let safe_len = self.safe_visible_prefix_len(&self.pending);
            let visible = self.pending[..safe_len].to_string();
            return (visible, self.tool_calls);
        }
        (String::new(), self.tool_calls)
    }

    /// If we are in tool mode, move safe content into the tool JSON buffer.
    /// If no JSON has started yet and the next character is not a JSON value
    /// start, treat the buffer as visible text instead.
    fn flush_non_tool_visible(&mut self, visible: &mut String) {
        if !self.in_tool_call {
            let safe_len = self.safe_visible_prefix_len(&self.pending);
            if safe_len > 0 {
                visible.push_str(&self.pending[..safe_len]);
                self.pending.drain(..safe_len);
            }
            return;
        }

        if self.tool_json.trim().is_empty() {
            let trimmed_start = self.pending.trim_start();
            if trimmed_start.is_empty() {
                return;
            }
            if !trimmed_start.starts_with('{') && !trimmed_start.starts_with('[') {
                // The next block is visible text, not another tool call.
                self.in_tool_call = false;
                let safe_len = self.safe_visible_prefix_len(&self.pending);
                if safe_len > 0 {
                    visible.push_str(&self.pending[..safe_len]);
                    self.pending.drain(..safe_len);
                }
                return;
            }
        }

        let safe_len = self.safe_visible_prefix_len(&self.pending);
        if safe_len > 0 {
            self.tool_json.push_str(&self.pending[..safe_len]);
            self.pending.drain(..safe_len);
        }
    }

    fn earliest_delimiter(&self, text: &str) -> Option<(usize, &'static str)> {
        let mut best: Option<(usize, &'static str)> = None;
        for &delimiter in MINIMAX_TOOL_DELIMITERS {
            if let Some(position) = text.find(delimiter) {
                if best
                    .as_ref()
                    .is_none_or(|(best_position, _)| position < *best_position)
                {
                    best = Some((position, delimiter));
                }
            }
        }
        best
    }

    /// Returns the longest prefix of `text` that cannot be part of an
    /// incomplete control delimiter at the end of the current buffer.
    fn safe_visible_prefix_len(&self, text: &str) -> usize {
        let mut safe_len = text.len();
        for &delimiter in MINIMAX_TOOL_DELIMITERS {
            for prefix_len in 1..delimiter.len() {
                let prefix = &delimiter[..prefix_len];
                if text.ends_with(prefix) {
                    safe_len = safe_len.min(text.len() - prefix_len);
                }
            }
        }
        safe_len
    }

    fn finish_tool_call(&mut self) -> Option<ToolCall> {
        let raw = std::mem::take(&mut self.tool_json);
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let parsed: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        let name = parsed.get("name")?.as_str()?.to_string();
        let arguments = match parsed.get("arguments") {
            Some(serde_json::Value::String(value)) => value.clone(),
            Some(value @ serde_json::Value::Object(_)) => serde_json::to_string(value).ok()?,
            _ => return None,
        };
        let id = parsed
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("minimax-content-call-{}", self.next_call_index));
        self.next_call_index += 1;
        Some(ToolCall::new(id, name, arguments))
    }
}

#[cfg(test)]
mod tests {
    use super::MinimaxContentToolCallParser;

    #[test]
    fn extracts_single_content_tool_call_and_keeps_visible_text() {
        let mut parser = MinimaxContentToolCallParser::new();
        let (visible, calls) = parser.push(
            "I'll look into it.]<|minimax|>[{\"name\":\"web_search\",\"arguments\":{\"query\":\"广元 影院\"}}]<|minimax|>[Done",
        );
        assert_eq!(visible, "I'll look into it.Done");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "web_search");
        assert!(calls[0]
            .function
            .arguments
            .contains("\"query\":\"广元 影院\""));
    }

    #[test]
    fn extracts_multiple_content_tool_calls() {
        let mut parser = MinimaxContentToolCallParser::new();
        let (visible, calls) = parser.push(
            "]<|minimax|>[{\"name\":\"web_search\",\"arguments\":{\"query\":\"a\"}}]<|minimax|>[{\"name\":\"web_search\",\"arguments\":{\"query\":\"b\"}}]<|minimax|>[",
        );
        assert_eq!(visible, "");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "minimax-content-call-0");
        assert_eq!(calls[1].id, "minimax-content-call-1");
    }

    #[test]
    fn handles_delimiters_split_across_stream_chunks() {
        let mut parser = MinimaxContentToolCallParser::new();
        let (v1, _) = parser.push("Hello ]<|mini");
        assert_eq!(v1, "Hello ");
        let (v2, calls) = parser.push(
            "max|>[{\"name\":\"web_search\",\"arguments\":{\"query\":\"x\"}}]<|minimax|>[ tail",
        );
        assert_eq!(v2, " tail");
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn discards_unclosed_tool_call_json_at_finish() {
        let mut parser = MinimaxContentToolCallParser::new();
        let (_, _) = parser.push("]<|minimax|>[{\"name\":\"web_search\",\"arguments\":{");
        let (visible, calls) = parser.finish();
        assert_eq!(visible, "");
        assert!(calls.is_empty());
    }

    #[test]
    fn supports_legacy_control_marker() {
        let mut parser = MinimaxContentToolCallParser::new();
        let (visible, calls) = parser.push(
            "<|minimax|>{\"name\":\"web_search\",\"arguments\":{\"query\":\"x\"}}<|minimax|>tail",
        );
        assert_eq!(visible, "tail");
        assert_eq!(calls.len(), 1);
    }
}
