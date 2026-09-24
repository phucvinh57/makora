use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::sqlite::SharedConnection;
use crate::core::category::CategoryTree;

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedTaxonomy {
    pub categories: Vec<SeedCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedCategory {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
    #[serde(default)]
    pub children: Vec<SeedCategory>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CategorizationResetOutcome {
    pub sessions_cleared: i64,
}

/// Fields for a category that does not exist yet.
#[derive(Debug, Clone)]
pub struct NewCategory {
    pub name: String,
    pub color: String,
    pub description: Option<String>,
    pub parent_id: Option<i64>,
}

/// An activity nobody has categorized yet, as the LLM needs to see it.
#[derive(Debug, Clone)]
pub struct UncategorizedActivity {
    pub id: i64,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
}

pub struct CategoryStore {
    conn: SharedConnection,
}

/// Turns the raw constraint errors into something the UI can show. Names only have to be
/// unique among siblings, so the same name elsewhere in the tree is fine.
fn write_error(err: rusqlite::Error, name: &str, parent_id: Option<i64>) -> anyhow::Error {
    if err.sqlite_error_code() != Some(rusqlite::ErrorCode::ConstraintViolation) {
        return err.into();
    }
    if err.to_string().contains("FOREIGN KEY") {
        return anyhow!("the parent category does not exist");
    }
    match parent_id {
        Some(_) => anyhow!("a subcategory named \"{name}\" already exists under this parent"),
        None => anyhow!("a top-level category named \"{name}\" already exists"),
    }
}

fn get_category(conn: &rusqlite::Connection, id: i64) -> Result<Option<Category>> {
    Ok(conn
        .query_row(
            "SELECT id, name, color, parent_id, description FROM categories WHERE id = ?1",
            params![id],
            |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    parent_id: row.get(3)?,
                    description: row.get(4)?,
                })
            },
        )
        .optional()?)
}

pub(super) fn load_category_tree(conn: &rusqlite::Connection) -> Result<CategoryTree> {
    let mut stmt = conn.prepare("SELECT id, parent_id FROM categories")?;
    let categories = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(CategoryTree::new(categories))
}

fn insert_seed_category(
    tx: &rusqlite::Transaction<'_>,
    category: &SeedCategory,
    parent_id: Option<i64>,
    category_ids: &mut std::collections::HashMap<String, i64>,
) -> Result<()> {
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM categories WHERE name = ?1 AND parent_id IS ?2",
            params![category.name, parent_id],
            |row| row.get(0),
        )
        .optional()?;
    let id = if let Some(id) = existing {
        id
    } else {
        tx.execute(
            "INSERT INTO categories (name, color, description, parent_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                category.name,
                category.color,
                category.description,
                parent_id
            ],
        )?;
        tx.last_insert_rowid()
    };
    if category_ids.insert(category.name.clone(), id).is_some() {
        bail!("seed category name is ambiguous: {}", category.name);
    }
    for child in &category.children {
        insert_seed_category(tx, child, Some(id), category_ids)?;
    }
    Ok(())
}

impl CategoryStore {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    /// Every category, flat — the caller (the UI) assembles the tree from `parent_id`.
    /// Siblings come out in the order they should be shown under their parent.
    pub fn list(&self) -> Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, color, parent_id, description FROM categories
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    parent_id: row.get(3)?,
                    description: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    #[allow(dead_code)]
    pub fn get(&self, id: i64) -> Result<Option<Category>> {
        let conn = self.conn.lock().unwrap();
        get_category(&conn, id)
    }

    pub fn create(&self, new: &NewCategory) -> Result<Category> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO categories (name, color, parent_id, description)
             VALUES (?1, ?2, ?3, ?4)",
            params![new.name, new.color, new.parent_id, new.description],
        )
        .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        Ok(Category {
            id: conn.last_insert_rowid(),
            name: new.name.clone(),
            color: new.color.clone(),
            parent_id: new.parent_id,
            description: new.description.clone(),
        })
    }

    /// Replaces every editable field.
    pub fn update(&self, id: i64, new: &NewCategory) -> Result<Category> {
        let conn = self.conn.lock().unwrap();
        if let Some(parent_id) = new.parent_id {
            // Re-parenting a category under itself or under one of its own descendants would
            // cut that whole branch loose from every root.
            let tree = load_category_tree(&conn)?;
            if parent_id == id || tree.descendants(id).contains(&parent_id) {
                bail!("a category cannot be moved inside itself");
            }
        }
        let updated = conn
            .execute(
                "UPDATE categories
                 SET name = ?1, color = ?2, description = ?3, parent_id = ?4
                 WHERE id = ?5",
                params![new.name, new.color, new.description, new.parent_id, id],
            )
            .map_err(|err| write_error(err, &new.name, new.parent_id))?;
        if updated == 0 {
            bail!("category {id} does not exist");
        }
        Ok(Category {
            id,
            name: new.name.clone(),
            color: new.color.clone(),
            parent_id: new.parent_id,
            description: new.description.clone(),
        })
    }

    pub fn seed_taxonomy_preview(seed_json: &str) -> Result<SeedTaxonomy> {
        serde_json::from_str(seed_json).context("parsing bundled seed taxonomy")
    }

    pub fn apply_seed_taxonomy(&self, seed_json: &str) -> Result<()> {
        let seed = Self::seed_taxonomy_preview(seed_json)?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut category_ids = std::collections::HashMap::new();
        for category in &seed.categories {
            insert_seed_category(&tx, category, None, &mut category_ids)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes a category *and everything nested under it* — a subcategory has no meaning
    /// without its parent, so the whole branch goes, taking its pins with it.
    pub fn delete(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let doomed = load_category_tree(&tx)?.deletion_order(id);
        if doomed.is_empty() {
            bail!("category {id} does not exist");
        }
        // Ids are integers, so inlining them is safe; one statement per table instead of one
        // per category.
        let ids = doomed
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        tx.execute_batch(&format!(
            "UPDATE activities SET category_id = NULL WHERE category_id IN ({ids});
             UPDATE activities SET ai_category_id = NULL WHERE ai_category_id IN ({ids});
             UPDATE sessions SET category_id = NULL WHERE category_id IN ({ids});
             DELETE FROM categories WHERE id IN ({ids});"
        ))?;
        tx.commit()?;
        Ok(())
    }

    /// Pins the activity behind each session row to `category_id` (or unpins with `None`).
    pub fn assign_sessions(&self, session_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut pin_activity = tx.prepare(
                "UPDATE activities SET category_id = ?1
                 WHERE id = (SELECT activity_id FROM sessions WHERE id = ?2)",
            )?;
            let mut pin_sessions = tx.prepare(
                "UPDATE sessions SET category_id = ?1
                 WHERE activity_id = (SELECT activity_id FROM sessions WHERE id = ?2)",
            )?;
            for session_id in session_ids {
                pin_activity.execute(params![category_id, session_id])?;
                pin_sessions.execute(params![category_id, session_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Clears every pinned category in one transaction while keeping the taxonomy and
    /// tracking history intact.
    pub fn reset_categorization(&self) -> Result<CategorizationResetOutcome> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let sessions_cleared: i64 = tx.query_row(
            "SELECT COUNT(*) FROM sessions s LEFT JOIN activities a ON a.id = s.activity_id
             WHERE COALESCE(a.category_id, s.category_id, a.ai_category_id) IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE activities SET category_id = NULL, ai_category_id = NULL",
            [],
        )?;
        tx.execute("UPDATE sessions SET category_id = NULL", [])?;

        tx.commit()?;
        Ok(CategorizationResetOutcome { sessions_cleared })
    }

    /// Activities with neither a manual pin nor an AI label, most recently seen first.
    pub fn get_uncategorized_activities(&self) -> Result<Vec<UncategorizedActivity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT a.id, a.app_name, a.url, COALESCE(NULLIF(a.display_title, ''), a.title)
             FROM activities a
             WHERE a.category_id IS NULL AND a.ai_category_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM sessions s
                               WHERE s.activity_id = a.id AND s.category_id IS NOT NULL)
             ORDER BY a.last_seen DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(UncategorizedActivity {
                    id: row.get(0)?,
                    app_name: row.get(1)?,
                    url: row.get(2)?,
                    title: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Stores AI labels as `(activity_id, category_id)`. A manual pin made in the meantime is
    /// untouched: reads prefer it over the AI label.
    pub fn assign_ai(&self, labels: &[(i64, i64)]) -> Result<()> {
        if labels.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE activities SET ai_category_id = ?2
                 WHERE id = ?1 AND category_id IS NULL
                   AND EXISTS (SELECT 1 FROM categories WHERE id = ?2)",
            )?;
            for (activity_id, category_id) in labels {
                stmt.execute(params![activity_id, category_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
