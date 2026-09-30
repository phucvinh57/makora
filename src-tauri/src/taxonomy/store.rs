//! Reading the category tree, and turning the database's constraint errors into ones about
//! categories.

use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use diesel::QueryResult;

use crate::db;
use crate::shared::ids::CategoryId;

use super::domain::CategoryTree;
use super::error::TaxonomyError;

impl From<DieselError> for TaxonomyError {
    fn from(err: DieselError) -> Self {
        Self::Internal(err.into())
    }
}

/// Loads every category's `(id, parent_id)` into a [`CategoryTree`] — shared with the review
/// queue and the history list, which both need to expand a category filter to its descendants.
pub fn load_tree(conn: &mut SqliteConnection) -> QueryResult<CategoryTree> {
    Ok(CategoryTree::new(db::categories::CategoriesRepo::get_parent_pairs(conn)?))
}

/// The category-level meaning of a failed insert or update.
pub fn translate_write_error(err: DieselError, name: &str, parent_id: Option<CategoryId>) -> TaxonomyError {
    match &err {
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => match parent_id {
            Some(_) => TaxonomyError::SubcategoryNameTaken(name.to_string()),
            None => TaxonomyError::TopLevelNameTaken(name.to_string()),
        },
        DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => TaxonomyError::ParentMissing,
        _ => err.into(),
    }
}
