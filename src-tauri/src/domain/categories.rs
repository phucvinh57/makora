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

use super::types::{Category, CategorizationResetOutcome, NewCategory};

pub struct Categories {
    db: Db,
}

impl Categories {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Every category, flat — the caller (the UI) assembles the tree from `parent_id`.
    /// Siblings come out in the order they should be shown under their parent.
    pub fn list(&self) -> Result<Vec<Category>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::categories::list_ordered(&mut conn)?.into_iter().map(to_category).collect())
    }

    #[allow(dead_code)]
    pub fn get(&self, id: i64) -> Result<Option<Category>> {
        let mut conn = self.db.lock().unwrap();
        Ok(db::categories::find(&mut conn, id)?.map(to_category))
    }

    pub fn create(&self, new: &NewCategory) -> Result<Category> {
        let mut conn = self.db.lock().unwrap();
        let row = db::categories::insert(
            &mut conn,
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

    /// Replaces every editable field.
    pub fn update(&self, id: i64, new: &NewCategory) -> Result<Category> {
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
        let row = db::categories::update(&mut conn, id, &changes)
            .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        let Some(row) = row else {
            bail!("category {id} does not exist");
        };
        Ok(to_category(row))
    }

    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its pins with it.
    pub fn delete(&self, id: i64) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| {
            let doomed = load_tree(conn)?.deletion_order(id);
            if doomed.is_empty() {
                bail!("category {id} does not exist");
            }
            db::activities::unpin_categories(conn, &doomed)?;
            db::ai_labels::delete_for_categories(conn, &doomed)?;
            db::categories::delete_many(conn, &doomed)?;
            Ok(())
        })
    }

    /// Pins the activity behind each session row to `category_id` (or unpins with `None`).
    pub fn assign_sessions(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            for activity_id in db::sessions::activity_ids_for(conn, session_ids)? {
                pin(conn, activity_id, category_id)?;
            }
            Ok(())
        })
        .map_err(Into::into)
    }

    /// Like [`assign_sessions`](Self::assign_sessions), for activities picked directly.
    pub fn assign_activities(&self, activity_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<()> {
            for &activity_id in activity_ids {
                pin(conn, activity_id, category_id)?;
            }
            Ok(())
        })
        .map_err(Into::into)
    }

    /// Clears every pinned category in one transaction while keeping the taxonomy and tracking
    /// history intact.
    pub fn reset_categorization(&self) -> Result<CategorizationResetOutcome> {
        let mut conn = self.db.lock().unwrap();
        conn.transaction(|conn| -> diesel::QueryResult<CategorizationResetOutcome> {
            let sessions_cleared = db::categories::count_sessions_with_category(conn)?;
            db::ai_labels::delete_all(conn)?;
            db::activities::clear_all_pins(conn)?;
            Ok(CategorizationResetOutcome { sessions_cleared })
        })
        .map_err(Into::into)
    }
}

/// Pins one activity to `category_id` (or unpins with `None`). Unpinning also rejects the AI
/// label it carries, if any, so "Uncategorized" sticks and the model learns from it.
fn pin(conn: &mut SqliteConnection, activity_id: i64, category_id: Option<i64>) -> diesel::QueryResult<()> {
    if category_id.is_none() {
        db::ai_labels::reject_active(conn, activity_id)?;
    }
    db::activities::set_category(conn, activity_id, category_id)
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
    Ok(CategoryTree::new(db::categories::parent_pairs(conn)?))
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
