//! How much of a page's address is sent: the `domain/path` the prompt already carries, the path
//! with its ids taken out, or the domain alone.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlPolicy {
    Full,
    #[default]
    StripIds,
    DomainOnly,
}

impl UrlPolicy {
    pub const ALL: [UrlPolicy; 3] = [Self::Full, Self::StripIds, Self::DomainOnly];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::StripIds => "strip_ids",
            Self::DomainOnly => "domain_only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|policy| policy.as_str() == value)
    }

    /// `location` is `domain/path`, as [`url_location`](crate::tracking::domain::source_key::url_location) gives it.
    pub fn apply(self, location: &str) -> String {
        let (domain, path) = location.split_once('/').unwrap_or((location, ""));
        match self {
            Self::Full => location.to_string(),
            Self::DomainOnly => domain.to_string(),
            Self::StripIds if path.is_empty() => domain.to_string(),
            Self::StripIds => {
                let path: Vec<&str> = path
                    .split('/')
                    .map(|segment| if is_id(segment) { "{id}" } else { segment })
                    .collect();
                format!("{domain}/{}", path.join("/"))
            }
        }
    }
}

/// A path segment that names one record rather than a kind of page: a number, a UUID, a hex
/// string, or a long run of letters and digits mixed.
fn is_id(segment: &str) -> bool {
    if segment.is_empty() {
        return false;
    }
    let has_digit = segment.chars().any(|c| c.is_ascii_digit());
    if segment.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let is_uuid = segment.len() == 36
        && segment
            .char_indices()
            .all(|(at, c)| if matches!(at, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() });
    if is_uuid {
        return true;
    }
    if segment.len() >= 8 && has_digit && segment.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if segment.len() < 16 || !has_digit || !segment.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) {
        return false;
    }
    // A slug such as `my-2024-review-of-the-year` is lowercase words; an id mixes cases.
    let mixed_case = segment.chars().any(|c| c.is_ascii_uppercase()) && segment.chars().any(|c| c.is_ascii_lowercase());
    mixed_case || (!segment.contains('-') && segment.chars().any(|c| c.is_ascii_alphabetic()))
}

/// `location` with `%XX` escapes decoded, so what is hidden behind them is found too.
pub fn decoded(location: &str) -> String {
    let bytes = location.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |offset: usize| bytes.get(at + offset).and_then(|b| (*b as char).to_digit(16));
        match (bytes[at], hex(1), hex(2)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_keeps_the_location() {
        assert_eq!(UrlPolicy::Full.apply("github.com/acme/api/pull/42"), "github.com/acme/api/pull/42");
    }

    #[test]
    fn domain_only_drops_the_path() {
        assert_eq!(UrlPolicy::DomainOnly.apply("github.com/acme/api/pull/42"), "github.com");
        assert_eq!(UrlPolicy::DomainOnly.apply("github.com"), "github.com");
    }

    #[test]
    fn strip_ids_replaces_record_segments() {
        let cases = [
            ("github.com/acme/api/pull/42", "github.com/acme/api/pull/{id}"),
            ("app.acme.com/orders/123e4567-e89b-12d3-a456-426614174000/edit", "app.acme.com/orders/{id}/edit"),
            ("github.com/acme/api/commit/3f786850e3", "github.com/acme/api/commit/{id}"),
            ("docs.google.com/document/d/1aBcD2eFgH3iJkL4mNoP", "docs.google.com/document/d/{id}"),
            ("docs.google.com/document/d/1aB-cD3eF_gH4iJ5kL6mN7oP8qR9", "docs.google.com/document/d/{id}"),
            ("blog.example.com/posts/my-2024-review-of-the-year", "blog.example.com/posts/my-2024-review-of-the-year"),
            ("blog.example.com/posts/my-2024-review", "blog.example.com/posts/my-2024-review"),
            ("example.com/v2/settings", "example.com/v2/settings"),
            ("example.com", "example.com"),
        ];
        for (location, expected) in cases {
            assert_eq!(UrlPolicy::StripIds.apply(location), expected, "{location}");
        }
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(decoded("app.acme.com/users/jane.doe%40acme.com/caf%C3%A9"), "app.acme.com/users/jane.doe@acme.com/café");
        assert_eq!(decoded("a.com/100%/x%zz"), "a.com/100%/x%zz");
    }

    #[test]
    fn policies_round_trip_their_names() {
        for policy in UrlPolicy::ALL {
            assert_eq!(UrlPolicy::parse(policy.as_str()), Some(policy));
        }
        assert_eq!(UrlPolicy::parse("nope"), None);
    }
}
