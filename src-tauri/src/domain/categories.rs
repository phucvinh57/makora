//! The category taxonomy: creating, editing and deleting categories. Filing activities under
//! them is [`crate::categorization`]'s job.

use anyhow::{anyhow, bail, Result};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::categorization;
use crate::db::models::{CategoryChanges, CategoryRow, NewCategoryRow};
use crate::db::{self, Db};
use crate::shared::ids::CategoryId;
use crate::taxonomy::domain::CategoryTree;

use super::types::{Category, NewCategory};

pub use super::traits::CategorySvc;

pub struct SqliteCategories {
    db: Db,
}

impl SqliteCategories {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl CategorySvc for SqliteCategories {
    fn get_list(&self) -> Result<Vec<Category>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::categories::CategoriesRepo::get_list(&mut *conn)?.into_iter().map(to_category).collect())
    }

    fn get(&self, id: CategoryId) -> Result<Option<Category>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::categories::CategoriesRepo::get(&mut *conn, id.into())?.map(to_category))
    }

    fn create(&self, new: &NewCategory) -> Result<Category> {
        let mut conn = self.db.lock().unwrap();
        let row = db::categories::CategoriesRepo::insert(
            &mut *conn,
            &NewCategoryRow {
                name: &new.name,
                color: &new.color,
                parent_id: new.parent_id.map(Into::into),
                description: new.description.as_deref(),
            },
        )
        .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        Ok(to_category(row))
    }

    fn update(&self, id: CategoryId, new: &NewCategory) -> Result<Category> {
        let mut conn = self.db.lock().unwrap();
        if let Some(parent_id) = new.parent_id {
            // Re-parenting a category under itself or under one of its own descendants would
            // cut that whole branch loose from every root.
            let tree = load_tree(&mut conn)?;
            if parent_id == id || tree.descendants(id.into()).contains(&parent_id.into()) {
                bail!("a category cannot be moved inside itself");
            }
        }
        let changes = CategoryChanges {
            name: &new.name,
            color: &new.color,
            parent_id: new.parent_id.map(Into::into),
            description: new.description.as_deref(),
        };
        let row = db::categories::CategoriesRepo::update(&mut *conn, id.into(), &changes)
            .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        let Some(row) = row else {
            bail!("category {id} does not exist");
        };
        Ok(to_category(row))
    }

    fn delete(&self, id: CategoryId) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| {
            let doomed = load_tree(conn)?.deletion_order(id.into());
            if doomed.is_empty() {
                bail!("category {id} does not exist");
            }
            // The branch's pins and AI labels go with it, in the same transaction.
            categorization::store::release_categories(conn, &doomed)?;
            db::categories::CategoriesRepo::delete_many(conn, &doomed)?;
            Ok(())
        })
    }
}

fn to_category(row: CategoryRow) -> Category {
    Category {
        id: row.id.into(),
        name: row.name,
        color: row.color,
        parent_id: row.parent_id.map(Into::into),
        description: row.description,
    }
}

/// Loads every category's `(id, parent_id)` into a [`CategoryTree`] — shared with
/// [`super::labeling`] and [`super::history`], which both need to expand a category filter to
/// its descendants.
pub(super) fn load_tree(conn: &mut SqliteConnection) -> Result<CategoryTree> {
    Ok(CategoryTree::new(db::categories::CategoriesRepo::get_parent_pairs(conn)?))
}

/// Turns the raw constraint errors into something the UI can show. Names only have to be
/// unique among siblings, so the same name elsewhere in the tree is fine.
fn write_error(err: DieselError, name: &str, parent_id: Option<CategoryId>) -> anyhow::Error {
    match &err {
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => match parent_id {
            Some(_) => anyhow!("a subcategory named \"{name}\" already exists under this parent"),
            None => anyhow!("a top-level category named \"{name}\" already exists"),
        },
        DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => {
            anyhow!("the parent category does not exist")
        }
        _ => err.into(),
    }
}
