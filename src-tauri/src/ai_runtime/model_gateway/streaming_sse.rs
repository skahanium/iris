//! Decode SSE TCP chunks without substituting invalid UTF-8.

use crate::error::{AppError, AppResult, ProviderErrorKind};

pub(super) fn decode_sse_utf8(pending: &mut Vec<u8>, chunk: &[u8]) -> AppResult<String> {
    pending.extend_from_slice(chunk);
    match std::str::from_utf8(pending) {
        Ok(text) => {
            let decoded = text.to_string();
            pending.clear();
            Ok(decoded)
        }
        Err(error) if error.error_len().is_some() => {
            pending.clear();
            Err(AppError::provider(
                ProviderErrorKind::InvalidResponse,
                "sse_invalid_utf8",
            ))
        }
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            let decoded = std::str::from_utf8(&pending[..valid_up_to])
                .unwrap_or_default()
                .to_string();
            pending.drain(..valid_up_to);
            Ok(decoded)
        }
    }
}

pub(super) fn finish_sse_utf8(pending: &[u8]) -> AppResult<()> {
    if pending.is_empty() {
        Ok(())
    } else {
        Err(AppError::provider(
            ProviderErrorKind::InvalidResponse,
            "sse_invalid_utf8",
        ))
    }
}
