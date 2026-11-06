use diesel::sqlite::SqliteConnection;
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
    fn create(&self, new: &NewCategory) -> Result<Category, TaxonomyError>;
    fn update(&self, id: CategoryId, new: &NewCategory) -> Result<Category, TaxonomyError>;
    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its pins with it.
    fn delete(&self, id: CategoryId) -> Result<(), TaxonomyError>;
    /// Folds `from` into `into`: its subcategories, pins, rules and AI labels move to `into`,
    /// then `from` is deleted. `into` may not sit inside `from`.
    fn merge(&self, from: CategoryId, into: CategoryId) -> Result<(), TaxonomyError>;
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

    fn merge(&self, from: CategoryId, into: CategoryId) -> Result<(), TaxonomyError> {
        self.db.write().transaction(|conn| merge(conn, from, into))
    }
}

fn merge(conn: &mut SqliteConnection, from: CategoryId, into: CategoryId) -> Result<(), TaxonomyError> {
    let tree = load_tree(conn)?;
    for id in [from, into] {
        if !tree.contains(id.into()) {
            return Err(TaxonomyError::NotFound(id));
        }
    }
    if from == into || tree.descendants(from.into()).contains(&into.into()) {
        return Err(TaxonomyError::Cycle);
    }
    db::categories::CategoriesRepo::reparent_children(conn, from.into(), into.into())?;
    categorization::store::move_category(conn, from.into(), into.into())?;
    db::categories::CategoriesRepo::delete_many(conn, &[from.into()])?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;
    use diesel::sql_types::BigInt;
    use diesel::{sql_query, QueryableByName, RunQueryDsl};

    use super::*;

    #[derive(QueryableByName)]
    struct Value {
        #[diesel(sql_type = BigInt)]
        value: i64,
    }

    fn value(conn: &mut SqliteConnection, sql: &str) -> i64 {
        sql_query(sql).get_result::<Value>(conn).unwrap().value
    }

    /// Work (1) > Code (2); Dev (3) > Tools (4). Activity 1 is pinned to Dev, 2 has an AI label in
    /// Dev, and Dev's rule files activity 3.
    fn conn() -> SqliteConnection {
        let mut conn = crate::db::open_in_memory();
        conn.batch_execute(
            "INSERT INTO categories (id, name, color, parent_id) VALUES
               (1, 'Work', '#000000', NULL), (2, 'Code', '#000000', 1),
               (3, 'Dev', '#000000', NULL), (4, 'Tools', '#000000', 3);
             INSERT INTO sources (id, kind, key, name) VALUES (1, 'exe', 'code', 'Code'), (2, 'exe', 'vim', 'Vim');
             INSERT INTO activities (id, source_id, path, title, category_id) VALUES
               (1, 1, '', 'a', 3), (2, 1, '', 'b', NULL), (3, 2, '', 'c', NULL);
             INSERT INTO ai_labels (activity_id, category_id) VALUES (2, 3);
             INSERT INTO category_rules (id, category_id, kind, value) VALUES (1, 3, 'app', 'vim');
             INSERT INTO rule_matches (activity_id, rule_id) VALUES (3, 1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn merging_moves_children_pins_labels_and_rules() {
        let mut conn = conn();
        merge(&mut conn, CategoryId(3), CategoryId(2)).unwrap();
        assert_eq!(value(&mut conn, "SELECT COUNT(*) AS value FROM categories WHERE id = 3"), 0);
        assert_eq!(value(&mut conn, "SELECT parent_id AS value FROM categories WHERE id = 4"), 2);
        assert_eq!(value(&mut conn, "SELECT category_id AS value FROM activities WHERE id = 1"), 2);
        assert_eq!(value(&mut conn, "SELECT category_id AS value FROM ai_labels WHERE activity_id = 2"), 2);
        assert_eq!(value(&mut conn, "SELECT category_id AS value FROM category_rules WHERE id = 1"), 2);
        assert_eq!(
            value(&mut conn, "SELECT category_id AS value FROM activity_category WHERE activity_id = 3"),
            2
        );
    }

    #[test]
    fn a_category_cannot_be_merged_into_its_own_branch() {
        let mut conn = conn();
        assert!(matches!(merge(&mut conn, CategoryId(1), CategoryId(2)), Err(TaxonomyError::Cycle)));
        assert!(matches!(merge(&mut conn, CategoryId(1), CategoryId(1)), Err(TaxonomyError::Cycle)));
        assert!(matches!(merge(&mut conn, CategoryId(9), CategoryId(1)), Err(TaxonomyError::NotFound(_))));
        assert_eq!(value(&mut conn, "SELECT COUNT(*) AS value FROM categories"), 4);
    }
}
