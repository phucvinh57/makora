//! The category taxonomy and the manual pins on activities. Pinning always wins over an AI
//! label ([`super::labeling`]); unpinning rejects whatever AI label the activity was carrying,
//! so "uncategorized" sticks and the model learns from it.

use anyhow::{anyhow, bail, Result};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;

use crate::category::CategoryTree;
use crate::db::models::{CategoryChanges, CategoryRow, NewCategoryRow};
use crate::db::{self, Db};

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

    fn get(&self, id: i64) -> Result<Option<Category>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::categories::CategoriesRepo::get(&mut *conn, id)?.map(to_category))
    }

    fn create(&self, new: &NewCategory) -> Result<Category> {
        let mut conn = self.db.lock().unwrap();
        let row = db::categories::CategoriesRepo::insert(
            &mut *conn,
            &NewCategoryRow {
                name: &new.name,
                color: &new.color,
                parent_id: new.parent_id,
                description: new.description.as_deref(),
            },
        )
        .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        Ok(to_category(row))
    }

    fn update(&self, id: i64, new: &NewCategory) -> Result<Category> {
        let mut conn = self.db.lock().unwrap();
        if let Some(parent_id) = new.parent_id {
            // Re-parenting a category under itself or under one of its own descendants would
            // cut that whole branch loose from every root.
            let tree = load_tree(&mut conn)?;
            if parent_id == id || tree.descendants(id).contains(&parent_id) {
                bail!("a category cannot be moved inside itself");
            }
        }
        let changes = CategoryChanges {
            name: &new.name,
            color: &new.color,
            parent_id: new.parent_id,
            description: new.description.as_deref(),
        };
        let row = db::categories::CategoriesRepo::update(&mut *conn, id, &changes)
            .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        let Some(row) = row else {
            bail!("category {id} does not exist");
        };
        Ok(to_category(row))
    }

    fn delete(&self, id: i64) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| {
            let doomed = load_tree(conn)?.deletion_order(id);
            if doomed.is_empty() {
                bail!("category {id} does not exist");
            }
            db::activities::ActivitiesRepo::unpin_categories(conn, &doomed)?;
            db::ai_labels::AiLabelsRepo::delete_for_categories(conn, &doomed)?;
            db::categories::CategoriesRepo::delete_many(conn, &doomed)?;
            Ok(())
        })
    }

    fn assign_sessions(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            let activity_ids = db::sessions::SessionsRepo::get_activity_ids_for(conn, session_ids)?;
            if activity_ids.is_empty() {
                return Ok(());
            }
            if category_id.is_none() {
                db::ai_labels::AiLabelsRepo::reject_active_many(conn, &activity_ids)?;
            }
            db::activities::ActivitiesRepo::set_category_many(conn, &activity_ids, category_id)
        })
        .map_err(Into::into)
    }

    fn assign_activities(&self, activity_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            // Unpinning also rejects the AI label each activity carries, if any, so
            // "Uncategorized" sticks and the model learns from it.
            if category_id.is_none() {
                db::ai_labels::AiLabelsRepo::reject_active_many(conn, activity_ids)?;
            }
            db::activities::ActivitiesRepo::set_category_many(conn, activity_ids, category_id)
        })
        .map_err(Into::into)
    }

    fn reset_categorization(&self) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            db::ai_labels::AiLabelsRepo::delete_all(conn)?;
            db::activities::ActivitiesRepo::clear_all_pins(conn)?;
            Ok(())
        })
        .map_err(Into::into)
    }
}

fn to_category(row: CategoryRow) -> Category {
    Category {
        id: row.id,
        name: row.name,
        color: row.color,
        parent_id: row.parent_id,
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
fn write_error(err: DieselError, name: &str, parent_id: Option<i64>) -> anyhow::Error {
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
