//! Unified current-fact domain DTOs.
//!
//! The field sets below are the Rust representation of Appendix D's required
//! fields for the five external current-fact domains. All user-visible strings
//! carry a conservative character budget; all source URLs must be HTTPS.

use serde::{Deserialize, Serialize};

pub(crate) use crate::ai_runtime::mcp_external_tools::DomainOperation;

/// A single resolvable public evidence source shared by every domain record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EvidenceOrigin {
    pub(crate) evidence_id: i64,
    pub(crate) provider_id: String,
    pub(crate) source_url: String,
    pub(crate) source_title: String,
    pub(crate) observed_at: String,
}

/// Weather current/forecast record.
///
/// `observation_time` is required for `weather.current`; `issue_time` is
/// required for `weather.forecast`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeatherRecord {
    pub(crate) location: String,
    pub(crate) condition: String,
    pub(crate) temperature: String,
    pub(crate) units: String,
    pub(crate) observation_time: Option<String>,
    pub(crate) issue_time: Option<String>,
    pub(crate) origin: EvidenceOrigin,
}

/// News search record.
///
/// `topic` and `location` are the `topic/location` required pair from
/// Appendix D; at least one must be present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewsRecord {
    pub(crate) title: String,
    pub(crate) publisher: String,
    pub(crate) published_at: String,
    pub(crate) topic: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) origin: EvidenceOrigin,
}

/// Finance quote/metrics/news record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FinanceRecord {
    pub(crate) instrument: String,
    pub(crate) asset_kind: String,
    pub(crate) currency: String,
    pub(crate) as_of: String,
    pub(crate) delay: String,
    pub(crate) value: String,
    pub(crate) origin: EvidenceOrigin,
}

/// Entertainment now-playing/upcoming/streaming record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EntertainmentRecord {
    pub(crate) title: String,
    pub(crate) region: String,
    pub(crate) channel: String,
    pub(crate) date: String,
    pub(crate) checked_at: String,
    pub(crate) origin: EvidenceOrigin,
}

/// Sports schedule/score record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SportsRecord {
    pub(crate) competition: String,
    pub(crate) participants: Vec<String>,
    pub(crate) start_time: String,
    pub(crate) status: String,
    pub(crate) score: Option<String>,
    pub(crate) checked_at: String,
    pub(crate) origin: EvidenceOrigin,
}

/// One normalized current-fact record from any of the five supported domains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum FreshDomainRecord {
    Weather(WeatherRecord),
    News(NewsRecord),
    Finance(FinanceRecord),
    Entertainment(EntertainmentRecord),
    Sports(SportsRecord),
}

pub(crate) const MAX_PROVIDER_ID_CHARS: usize = 128;
pub(crate) const MAX_SOURCE_URL_CHARS: usize = 2_048;
pub(crate) const MAX_SOURCE_TITLE_CHARS: usize = 512;
pub(crate) const MAX_LOCATION_CHARS: usize = 256;
pub(crate) const MAX_CONDITION_CHARS: usize = 256;
pub(crate) const MAX_TEMPERATURE_CHARS: usize = 64;
pub(crate) const MAX_UNITS_CHARS: usize = 32;
pub(crate) const MAX_TITLE_CHARS: usize = 512;
pub(crate) const MAX_PUBLISHER_CHARS: usize = 256;
pub(crate) const MAX_TOPIC_CHARS: usize = 256;
pub(crate) const MAX_INSTRUMENT_CHARS: usize = 256;
pub(crate) const MAX_ASSET_KIND_CHARS: usize = 64;
pub(crate) const MAX_CURRENCY_CHARS: usize = 32;
pub(crate) const MAX_DELAY_CHARS: usize = 64;
pub(crate) const MAX_VALUE_CHARS: usize = 128;
pub(crate) const MAX_REGION_CHARS: usize = 128;
pub(crate) const MAX_CHANNEL_CHARS: usize = 256;
pub(crate) const MAX_DATE_CHARS: usize = 32;
pub(crate) const MAX_COMPETITION_CHARS: usize = 256;
pub(crate) const MAX_PARTICIPANT_CHARS: usize = 128;
pub(crate) const MAX_PARTICIPANTS: usize = 32;
pub(crate) const MAX_STATUS_CHARS: usize = 64;
pub(crate) const MAX_SCORE_CHARS: usize = 64;
pub(crate) const MAX_TIME_CHARS: usize = 64;
