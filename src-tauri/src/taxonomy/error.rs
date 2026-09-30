use crate::shared::ids::CategoryId;

/// Why a change to the category tree was refused.
#[derive(Debug, thiserror::Error)]
pub enum TaxonomyError {
    /// Names only have to be unique among siblings, so the same name elsewhere is fine.
    #[error("a top-level category named \"{0}\" already exists")]
    TopLevelNameTaken(String),
    #[error("a subcategory named \"{0}\" already exists under this parent")]
    SubcategoryNameTaken(String),
    #[error("the parent category does not exist")]
    ParentMissing,
    /// Re-parenting a category under itself or under one of its own descendants would cut that
    /// whole branch loose from every root.
    #[error("a category cannot be moved inside itself")]
    Cycle,
    #[error("category {0} does not exist")]
    NotFound(CategoryId),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
