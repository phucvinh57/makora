//! One type per kind of row id, so an activity id can no longer be passed where a category id
//! is expected. On the wire each is still the bare number (`serde(transparent)`), so the
//! frontend contract is unchanged; the repository layer keeps working in plain `i64`.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl From<i64> for $name {
            fn from(id: i64) -> Self {
                Self(id)
            }
        }

        impl From<$name> for i64 {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(
    SessionId
);
id_type!(
    ActivityId
);
id_type!(
    CategoryId
);

id_type!(
    RuleId
);

/// The id of a remote LLM provider: the key of its entry in the settings and of its API key in the
/// keychain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ProviderId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub fn raw<T: Copy + Into<i64>>(ids: &[T]) -> Vec<i64> {
    ids.iter().map(|&id| id.into()).collect()
}
