//! A failed core call as the UI shows it: a [`ErrorCode`] to branch on and a message fit to show.
//! The contexts' own errors are turned into it here, and only here.

use std::fmt;

use makora_lib::ai::model_download::ModelDownloadError;
use makora_lib::ai::providers::ProviderError;
use makora_lib::categorization::application::ai_run::AiRunError;
use makora_lib::categorization::application::rules::RuleError;
use makora_lib::reporting::query::QueryError;
use makora_lib::settings::SettingsError;
use makora_lib::taxonomy::TaxonomyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NameTaken,
    ParentMissing,
    Cycle,
    NotFound,
    InvalidRule,
    DuplicateRule,
    CategoryMissing,
    AlreadyRunning,
    NoCategories,
    InvalidInput,
    ProviderError,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    pub code: ErrorCode,
    pub message: String,
}

pub type UiResult<T> = Result<T, UiError>;

impl UiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UiError {}

impl From<anyhow::Error> for UiError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal(format!("{err:#}"))
    }
}

impl From<TaxonomyError> for UiError {
    fn from(err: TaxonomyError) -> Self {
        let code = match &err {
            TaxonomyError::TopLevelNameTaken(_) | TaxonomyError::SubcategoryNameTaken(_) => {
                ErrorCode::NameTaken
            }
            TaxonomyError::InvalidName(_) => ErrorCode::InvalidInput,
            TaxonomyError::ParentMissing => ErrorCode::ParentMissing,
            TaxonomyError::Cycle => ErrorCode::Cycle,
            TaxonomyError::NotFound(_) => ErrorCode::NotFound,
            TaxonomyError::Internal(_) => ErrorCode::Internal,
        };
        Self::new(code, err.to_string())
    }
}

impl From<RuleError> for UiError {
    fn from(err: RuleError) -> Self {
        let code = match &err {
            RuleError::Invalid(_) => ErrorCode::InvalidRule,
            RuleError::Duplicate => ErrorCode::DuplicateRule,
            RuleError::CategoryMissing => ErrorCode::CategoryMissing,
            RuleError::NotFound(_) => ErrorCode::NotFound,
            RuleError::Failed(_) => ErrorCode::Internal,
        };
        match err {
            RuleError::Failed(cause) => Self::new(code, format!("{cause:#}")),
            other => Self::new(code, other.to_string()),
        }
    }
}

impl From<AiRunError> for UiError {
    fn from(err: AiRunError) -> Self {
        match err {
            AiRunError::AlreadyRunning => Self::new(ErrorCode::AlreadyRunning, err.to_string()),
            AiRunError::NoCategories => Self::new(ErrorCode::NoCategories, err.to_string()),
            AiRunError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}

impl From<SettingsError> for UiError {
    fn from(err: SettingsError) -> Self {
        match err {
            SettingsError::Invalid(message) => Self::invalid_input(message),
            SettingsError::InvalidRule(err) => Self::new(ErrorCode::InvalidRule, err.to_string()),
            SettingsError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}

impl From<ModelDownloadError> for UiError {
    fn from(err: ModelDownloadError) -> Self {
        match err {
            ModelDownloadError::AlreadyRunning => {
                Self::new(ErrorCode::AlreadyRunning, err.to_string())
            }
            ModelDownloadError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}

impl From<ProviderError> for UiError {
    fn from(err: ProviderError) -> Self {
        match err {
            ProviderError::NotFound(_) => Self::new(ErrorCode::NotFound, err.to_string()),
            ProviderError::Settings(err) => err.into(),
            ProviderError::Keychain(_) => Self::internal(err.to_string()),
            ProviderError::Remote(_) => Self::new(ErrorCode::ProviderError, err.to_string()),
        }
    }
}

impl From<QueryError> for UiError {
    fn from(err: QueryError) -> Self {
        match err {
            QueryError::Invalid(message) => Self::invalid_input(message),
            QueryError::Failed(cause) => cause.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_errors_keep_their_code() {
        let err = UiError::from(TaxonomyError::Cycle);
        assert_eq!(err.code, ErrorCode::Cycle);
        assert_eq!(err.to_string(), TaxonomyError::Cycle.to_string());
    }

    #[test]
    fn a_failed_run_is_internal_with_its_cause() {
        let err = UiError::from(AiRunError::Failed(anyhow::anyhow!("model missing")));
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("model missing"));
    }

    #[test]
    fn invalid_settings_are_invalid_input() {
        let err = UiError::from(SettingsError::Invalid("bad".into()));
        assert_eq!(err, UiError::invalid_input("bad"));
    }
}
