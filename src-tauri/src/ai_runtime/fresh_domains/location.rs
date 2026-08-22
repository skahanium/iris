//! Confirmed common-location resolution for current-fact domains.
//!
//! Location may come from the current request's explicit location or from
//! confirmed global memory keys (`location.city`, `location.province`,
//! `location.country`). Vault-scoped memories, Web content, IP-derived strings
//! and similar-looking keys are never treated as confirmed location.

/// One confirmed location fragment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConfirmedLocation {
    pub(crate) city: Option<String>,
    pub(crate) province: Option<String>,
    pub(crate) country: Option<String>,
}

/// A read-only projection of one `ai_memories` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiMemory {
    pub(crate) key: String,
    pub(crate) content: String,
    pub(crate) scope: String,
}

/// A geographical scope used when constructing a domain request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocationScope {
    City,
    Province,
    Country,
}

impl LocationScope {
    /// Return the confirmed value for this scope, if any.
    pub(crate) fn value(self, location: &ConfirmedLocation) -> Option<&str> {
        let value = match self {
            Self::City => location.city.as_deref(),
            Self::Province => location.province.as_deref(),
            Self::Country => location.country.as_deref(),
        };
        value.filter(|value| !value.trim().is_empty())
    }

    /// Return the next wider scope, or `None` when country is the widest.
    pub(crate) fn next(self, location: &ConfirmedLocation) -> Option<Self> {
        match self {
            Self::City
                if location
                    .province
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty()) =>
            {
                Some(Self::Province)
            }
            Self::Province
                if location
                    .country
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty()) =>
            {
                Some(Self::Country)
            }
            Self::City | Self::Province | Self::Country => None,
        }
    }
}

/// Resolve the confirmed location for a current-fact request.
///
/// The explicit location wins per field; global memory only fills fields the
/// user did not already confirm in this request.
pub(crate) fn resolve_confirmed_location(
    explicit: Option<&ConfirmedLocation>,
    memories: &[AiMemory],
) -> ConfirmedLocation {
    let mut city = explicit.and_then(|location| clean(location.city.as_ref()));
    let mut province = explicit.and_then(|location| clean(location.province.as_ref()));
    let mut country = explicit.and_then(|location| clean(location.country.as_ref()));

    for memory in memories {
        if memory.scope != "global" {
            continue;
        }
        let content = memory.content.trim();
        if content.is_empty() {
            continue;
        }
        match memory.key.as_str() {
            "location.city" if city.is_none() => city = Some(content.to_string()),
            "location.province" if province.is_none() => province = Some(content.to_string()),
            "location.country" if country.is_none() => country = Some(content.to_string()),
            _ => {}
        }
    }

    ConfirmedLocation {
        city,
        province,
        country,
    }
}

/// Return the first confirmed scope using the fixed city → province → country
/// precedence.
pub(crate) fn first_location_scope(location: &ConfirmedLocation) -> Option<LocationScope> {
    [
        LocationScope::City,
        LocationScope::Province,
        LocationScope::Country,
    ]
    .into_iter()
    .find(|scope| scope.value(location).is_some())
}

fn clean(value: Option<&String>) -> Option<String> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
