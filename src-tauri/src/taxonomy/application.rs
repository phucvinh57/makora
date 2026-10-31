use diesel::Connection;

use crate::categorization;
use crate::db::models::{CategoryChanges, CategoryRow, NewCategoryRow};
use crate::db::{self, Db};
use crate::domain::types::{Category, NewCategory};
use crate::shared::ids::CategoryId;

use super::domain::category_name;
use super::error::TaxonomyError;
use super::store::{load_tree, translate_write_error};

pub trait CategorySvc: Send + Sync {
    /// Every category, flat — the caller (the UI) assembles the tree from `parent_id`.
    /// Siblings come out in the order they should be shown under their parent.
    fn get_list(&self) -> Result<Vec<Category>, TaxonomyError>;
    #[allow(dead_code)]
    fn get(&self, id: CategoryId) -> Result<Option<Category>, TaxonomyError>;
    fn create(&self, new: &NewCategory) -> Result<Category, TaxonomyError>;
    fn update(&self, id: CategoryId, new: &NewCategory) -> Result<Category, TaxonomyError>;
    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its pins with it.
    fn delete(&self, id: CategoryId) -> Result<(), TaxonomyError>;
}

pub struct SqliteCategories {
    db: Db,
}

impl SqliteCategories {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl CategorySvc for SqliteCategories {
    fn get_list(&self) -> Result<Vec<Category>, TaxonomyError> {
        let mut conn = self.db.read()?;
        Ok(db::categories::CategoriesRepo::get_list(&mut *conn)?
            .into_iter()
            .map(to_category)
            .collect())
    }

    fn get(&self, id: CategoryId) -> Result<Option<Category>, TaxonomyError> {
        let mut conn = self.db.read()?;
        Ok(db::categories::CategoriesRepo::get(&mut *conn, id.into())?.map(to_category))
    }

    fn create(&self, new: &NewCategory) -> Result<Category, TaxonomyError> {
        let name = category_name(&new.name)?;
        let mut conn = self.db.write();
        let row = db::categories::CategoriesRepo::insert(
            &mut *conn,
            &NewCategoryRow {
                name: &name,
                color: &new.color,
                parent_id: new.parent_id.map(Into::into),
                description: new.description.as_deref(),
            },
        )
        .map_err(|err| translate_write_error(err, &name, new.parent_id))?;
        Ok(to_category(row))
    }

    fn update(&self, id: CategoryId, new: &NewCategory) -> Result<Category, TaxonomyError> {
        let name = category_name(&new.name)?;
        let mut conn = self.db.write();
        if let Some(parent_id) = new.parent_id {
            let tree = load_tree(&mut conn)?;
            if parent_id == id || tree.descendants(id.into()).contains(&parent_id.into()) {
                return Err(TaxonomyError::Cycle);
            }
        }
        let changes = CategoryChanges {
            name: &name,
            color: &new.color,
            parent_id: new.parent_id.map(Into::into),
            description: new.description.as_deref(),
        };
        let row = db::categories::CategoriesRepo::update(&mut *conn, id.into(), &changes)
            .map_err(|err| translate_write_error(err, &name, new.parent_id))?;
        row.map(to_category).ok_or(TaxonomyError::NotFound(id))
    }

    fn delete(&self, id: CategoryId) -> Result<(), TaxonomyError> {
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            let doomed = load_tree(conn)?.deletion_order(id.into());
            if doomed.is_empty() {
                return Err(TaxonomyError::NotFound(id));
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
