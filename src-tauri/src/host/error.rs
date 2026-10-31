//! What a failed command tells the frontend: a stable `code` to branch on and a `message` fit to
//! show. The contexts' own errors are turned into it here, and only here.

use serde::Serialize;

use crate::ai::model_download::ModelDownloadError;
use crate::categorization::application::ai_run::AiRunError;
use crate::categorization::application::rules::RuleError;
use crate::settings::SettingsError;
use crate::taxonomy::TaxonomyError;

#[derive(Debug, Serialize)]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
}

impl IpcError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new("invalid_input", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }
}

impl From<anyhow::Error> for IpcError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal(err.to_string())
    }
}

impl From<TaxonomyError> for IpcError {
    fn from(err: TaxonomyError) -> Self {
        let code = match &err {
            TaxonomyError::TopLevelNameTaken(_) | TaxonomyError::SubcategoryNameTaken(_) => "name_taken",
            TaxonomyError::InvalidName(_) => "invalid_input",
            TaxonomyError::ParentMissing => "parent_missing",
            TaxonomyError::Cycle => "cycle",
            TaxonomyError::NotFound(_) => "not_found",
            TaxonomyError::Internal(_) => "internal",
        };
        Self::new(code, err.to_string())
    }
}

impl From<RuleError> for IpcError {
    fn from(err: RuleError) -> Self {
        let code = match &err {
            RuleError::Invalid(_) => "invalid_rule",
            RuleError::Duplicate => "duplicate_rule",
            RuleError::CategoryMissing => "category_missing",
            RuleError::NotFound(_) => "not_found",
            RuleError::Failed(_) => "internal",
        };
        match err {
            RuleError::Failed(cause) => Self::new(code, format!("{cause:#}")),
            other => Self::new(code, other.to_string()),
        }
    }
}

impl From<AiRunError> for IpcError {
    fn from(err: AiRunError) -> Self {
        match err {
            AiRunError::AlreadyRunning => Self::new("already_running", err.to_string()),
            AiRunError::NoCategories => Self::new("no_categories", err.to_string()),
            AiRunError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}

impl From<SettingsError> for IpcError {
    fn from(err: SettingsError) -> Self {
        match err {
            SettingsError::Invalid(message) => Self::invalid_input(message),
            SettingsError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}

impl From<ModelDownloadError> for IpcError {
    fn from(err: ModelDownloadError) -> Self {
        match err {
            ModelDownloadError::AlreadyRunning => Self::new("already_running", err.to_string()),
            ModelDownloadError::Failed(cause) => Self::internal(format!("{cause:#}")),
        }
    }
}
