//! What a failed command tells the frontend: a stable `code` to branch on and a `message` fit to
//! show. The contexts' own errors are turned into it here, and only here.

use serde::Serialize;

use crate::ai::model_download::ModelDownloadError;
use crate::categorization::application::ai_run::AiRunError;
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

    /// The request itself was malformed: a filter or timestamp that cannot be read.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new("invalid_input", message)
    }

    /// Anything that is not the caller's doing: I/O, the database, the model.
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
            TaxonomyError::ParentMissing => "parent_missing",
            TaxonomyError::Cycle => "cycle",
            TaxonomyError::NotFound(_) => "not_found",
            TaxonomyError::Internal(_) => "internal",
        };
        Self::new(code, err.to_string())
    }
}

impl From<AiRunError> for IpcError {
    fn from(err: AiRunError) -> Self {
        match err {
            AiRunError::AlreadyRunning => Self::new("already_running", err.to_string()),
            AiRunError::Failed(cause) => Self::internal(format!("{cause:#}")),
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
