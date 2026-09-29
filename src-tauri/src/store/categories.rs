use anyhow::{anyhow, bail, Result};
use rusqlite::{named_params, params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::sqlite::SharedConnection;
use crate::category::CategoryTree;

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
    pub description: Option<String>,
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

/// An activity the LLM is asked to label, as it needs to see it.
#[derive(Debug, Clone)]
pub struct UncategorizedActivity {
    pub id: i64,
    pub key_kind: String,
    pub key: String,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
}

/// An activity the user has judged, either by pinning it or by rejecting an AI label. These
/// are the examples the LLM learns from.
#[derive(Debug, Clone)]
pub struct LabeledActivity {
    pub key_kind: String,
    pub key: String,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub category_id: Option<i64>,
    pub ai_category_id: Option<i64>,
    pub ai_rejected_category_id: Option<i64>,
}

/// An AI label waiting for the user to accept, change or reject it.
#[derive(Debug, Clone, Serialize)]
pub struct AiReviewRow {
    pub activity_id: i64,
    pub app_name: String,
    pub url: Option<String>,
    pub title: String,
    pub category_id: i64,
    pub category_name: String,
    pub category_color: String,
    pub total_seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewPage {
    pub rows: Vec<AiReviewRow>,
    pub total: i64,
}

/// AI labels of one source (an executable or a domain) that suggest the same category, so one
/// decision can cover all of them.
#[derive(Debug, Clone, Serialize)]
pub struct AiReviewGroup {
    pub key_kind: String,
    pub key: String,
    pub app_name: String,
    pub category_id: i64,
    pub category_name: String,
    pub category_color: String,
    pub activity_count: i64,
    pub total_seconds: f64,
    /// The least sure the model was about any member; `None` when unknown.
    pub min_confidence: Option<f64>,
    pub activity_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiReviewGroupPage {
    pub groups: Vec<AiReviewGroup>,
    pub total: i64,
}

/// Which pending AI labels the review queue shows.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFilter {
    /// Only labels in this category or under it.
    pub category: Option<i64>,
    /// Only these activities, e.g. the members of one group.
    pub activities: Option<Vec<i64>>,
    /// Hide labels the model was confident about.
    pub uncertain_only: bool,
    /// Hide activities (or groups) tracked for less than this, in seconds.
    pub min_seconds: f64,
}

/// How much the user has taught the model so far.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FeedbackStats {
    pub corrected: i64,
    pub confirmed: i64,
    pub pinned: i64,
    pub rejected: i64,
    pub pending_review: i64,
    /// Tracked time of the activities still waiting for review.
    pub pending_seconds: f64,
    /// Tracked time of the AI labels the user has already judged.
    pub reviewed_seconds: f64,
}

/// An activity no session of which is pinned on its own: the only ones an AI label can show
/// through for, since a session pin outranks it.
const NO_SESSION_PIN: &str = "NOT EXISTS (SELECT 1 FROM sessions s
                                          WHERE s.activity_id = a.id AND s.category_id IS NOT NULL)";

/// Labels the model was at least this sure of are left out of the default review queue.
const CONFIDENT: f64 = 0.85;

/// Tracked seconds of the activity `a` in a query that left-joins `sessions s` onto it.
const SECONDS: &str =
    "COALESCE(SUM((julianday(s.end_time) - julianday(s.start_time)) * 86400.0), 0.0)";

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

/// Pins one activity and every session of it. Clearing a pin also rejects the AI label that
/// would otherwise show through, so "Uncategorized" sticks and the model learns from it.
fn pin_activity(
    tx: &rusqlite::Transaction<'_>,
    activity_id: i64,
    category_id: Option<i64>,
) -> Result<()> {
    if category_id.is_none() {
        tx.execute(
            "UPDATE activities
             SET ai_rejected_category_id = ai_category_id, ai_category_id = NULL,
                 ai_confidence = NULL
             WHERE id = ?1 AND ai_category_id IS NOT NULL",
            params![activity_id],
        )?;
    }
    tx.execute(
        "UPDATE activities SET category_id = ?1 WHERE id = ?2",
        params![category_id, activity_id],
    )?;
    tx.execute(
        "UPDATE sessions SET category_id = ?1 WHERE activity_id = ?2",
        params![category_id, activity_id],
    )?;
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
             UPDATE activities SET ai_category_id = NULL, ai_confidence = NULL
                 WHERE ai_category_id IN ({ids});
             UPDATE activities SET ai_rejected_category_id = NULL
                 WHERE ai_rejected_category_id IN ({ids});
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
        for session_id in session_ids {
            let activity_id: Option<Option<i64>> = tx
                .query_row(
                    "SELECT activity_id FROM sessions WHERE id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(Some(activity_id)) = activity_id {
                pin_activity(&tx, activity_id, category_id)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Like [`assign_sessions`](Self::assign_sessions), for activities picked directly.
    pub fn assign_activities(&self, activity_ids: &[i64], category_id: Option<i64>) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for activity_id in activity_ids {
            pin_activity(&tx, *activity_id, category_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Turns each activity's AI label into a manual pin.
    pub fn accept_ai_labels(&self, activity_ids: &[i64]) -> Result<()> {
        if activity_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for activity_id in activity_ids {
            let accepted = tx.execute(
                "UPDATE activities SET category_id = ai_category_id
                 WHERE id = ?1 AND category_id IS NULL AND ai_category_id IS NOT NULL",
                params![activity_id],
            )?;
            if accepted > 0 {
                tx.execute(
                    "UPDATE sessions
                     SET category_id = (SELECT category_id FROM activities WHERE id = ?1)
                     WHERE activity_id = ?1",
                    params![activity_id],
                )?;
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
            "UPDATE activities
             SET category_id = NULL, ai_category_id = NULL, ai_rejected_category_id = NULL,
                 ai_confidence = NULL",
            [],
        )?;
        tx.execute("UPDATE sessions SET category_id = NULL", [])?;

        tx.commit()?;
        Ok(CategorizationResetOutcome { sessions_cleared })
    }

    /// Activities for the LLM to label, most recently seen first. Normally those with neither a
    /// pin nor an AI label nor a rejected one; with `recheck`, those carrying an AI label the
    /// user has not confirmed.
    pub fn get_activities_to_label(&self, recheck: bool) -> Result<Vec<UncategorizedActivity>> {
        let wanted = if recheck {
            "a.ai_category_id IS NOT NULL"
        } else {
            "a.ai_category_id IS NULL AND a.ai_rejected_category_id IS NULL"
        };
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT a.id, a.key_kind, a.key, a.app_name, a.url,
                    COALESCE(NULLIF(a.display_title, ''), a.title)
             FROM activities a
             WHERE a.category_id IS NULL AND {wanted} AND {NO_SESSION_PIN}
             ORDER BY a.last_seen DESC"
        ))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(UncategorizedActivity {
                    id: row.get(0)?,
                    key_kind: row.get(1)?,
                    key: row.get(2)?,
                    app_name: row.get(3)?,
                    url: row.get(4)?,
                    title: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Stores AI labels as `(activity_id, category_id, confidence)`; `None` clears a label the
    /// model no longer stands by. A manual pin made in the meantime is untouched: reads prefer it over
    /// the AI label. Returns how many labels actually changed.
    pub fn assign_ai(&self, labels: &[(i64, Option<i64>, f64)]) -> Result<usize> {
        if labels.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut set = tx.prepare(
                "UPDATE activities SET ai_category_id = ?2
                 WHERE id = ?1 AND category_id IS NULL AND ai_category_id IS NOT ?2
                   AND EXISTS (SELECT 1 FROM categories WHERE id = ?2)",
            )?;
            // Not counted as a change: a re-check that lands on the same category only
            // refreshes how sure the model was.
            let mut confidence_stmt = tx.prepare(
                "UPDATE activities SET ai_confidence = ?2
                 WHERE id = ?1 AND category_id IS NULL AND ai_category_id IS NOT NULL",
            )?;
            let mut clear = tx.prepare(
                "UPDATE activities SET ai_category_id = NULL, ai_confidence = NULL
                 WHERE id = ?1 AND category_id IS NULL AND ai_category_id IS NOT NULL",
            )?;
            for (activity_id, category_id, confidence) in labels {
                changed += match category_id {
                    Some(category_id) => {
                        let n = set.execute(params![activity_id, category_id])?;
                        confidence_stmt.execute(params![activity_id, confidence])?;
                        n
                    }
                    None => clear.execute(params![activity_id])?,
                };
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Every activity the user has pinned or rejected an AI label for, newest first.
    pub fn labeled_activities(&self) -> Result<Vec<LabeledActivity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key_kind, key, app_name, url, COALESCE(NULLIF(display_title, ''), title),
                    category_id, ai_category_id, ai_rejected_category_id
             FROM activities
             WHERE category_id IS NOT NULL OR ai_rejected_category_id IS NOT NULL
             ORDER BY last_seen DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(LabeledActivity {
                    key_kind: row.get(0)?,
                    key: row.get(1)?,
                    app_name: row.get(2)?,
                    url: row.get(3)?,
                    title: row.get(4)?,
                    category_id: row.get(5)?,
                    ai_category_id: row.get(6)?,
                    ai_rejected_category_id: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The `WHERE` clause for pending AI labels under `filter`, or `None` when the category
    /// filter can match nothing. Binds `:uncertain` and `:confident`.
    fn pending_clause(conn: &rusqlite::Connection, filter: &ReviewFilter) -> Result<Option<String>> {
        let scope = match filter.category {
            Some(id) => {
                let ids = load_category_tree(conn)?.descendants(id);
                if ids.is_empty() {
                    return Ok(None);
                }
                // Ids are integers, so inlining them is safe.
                let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
                format!("AND a.ai_category_id IN ({ids})")
            }
            None => String::new(),
        };
        let only = match &filter.activities {
            Some(ids) => format!(
                "AND a.id IN ({})",
                ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
            ),
            None => String::new(),
        };
        Ok(Some(format!(
            "a.category_id IS NULL AND a.ai_category_id IS NOT NULL AND {NO_SESSION_PIN}
             {scope} {only}
             AND (:uncertain = 0 OR a.ai_confidence IS NULL OR a.ai_confidence < :confident)"
        )))
    }

    /// AI labels awaiting review, longest-tracked first so the ones that matter come up first.
    /// Filtering by a category includes everything under it.
    pub fn ai_review_page(
        &self,
        page: i64,
        page_size: i64,
        filter: &ReviewFilter,
    ) -> Result<AiReviewPage> {
        let conn = self.conn.lock().unwrap();
        let Some(pending) = Self::pending_clause(&conn, filter)? else {
            return Ok(AiReviewPage {
                rows: Vec::new(),
                total: 0,
            });
        };
        let total: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM (
                     SELECT a.id FROM activities a
                     LEFT JOIN sessions s ON s.activity_id = a.id AND s.end_time IS NOT NULL
                     WHERE {pending}
                     GROUP BY a.id
                     HAVING {SECONDS} >= :min)"
            ),
            named_params! {
                ":uncertain": filter.uncertain_only,
                ":confident": CONFIDENT, ":min": filter.min_seconds,
            },
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(&format!(
            "SELECT a.id, a.app_name, a.url, COALESCE(NULLIF(a.display_title, ''), a.title),
                    cat.id, cat.name, cat.color, {SECONDS}, COUNT(s.id)
             FROM activities a
             JOIN categories cat ON cat.id = a.ai_category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND s.end_time IS NOT NULL
             WHERE {pending}
             GROUP BY a.id
             HAVING {SECONDS} >= :min
             ORDER BY 8 DESC, a.id
             LIMIT :limit OFFSET :offset"
        ))?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":uncertain": filter.uncertain_only,
                    ":confident": CONFIDENT, ":min": filter.min_seconds,
                    ":limit": page_size, ":offset": page * page_size,
                },
                |row| {
                    Ok(AiReviewRow {
                        activity_id: row.get(0)?,
                        app_name: row.get(1)?,
                        url: row.get(2)?,
                        title: row.get(3)?,
                        category_id: row.get(4)?,
                        category_name: row.get(5)?,
                        category_color: row.get(6)?,
                        total_seconds: row.get(7)?,
                        session_count: row.get(8)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(AiReviewPage { rows, total })
    }

    /// The pending AI labels folded into one row per source and suggested category, the
    /// longest-tracked first. `filter.activities` is ignored: a group picks its own members.
    pub fn ai_review_groups(
        &self,
        page: i64,
        page_size: i64,
        filter: &ReviewFilter,
    ) -> Result<AiReviewGroupPage> {
        let conn = self.conn.lock().unwrap();
        let filter = ReviewFilter {
            activities: None,
            ..filter.clone()
        };
        let Some(pending) = Self::pending_clause(&conn, &filter)? else {
            return Ok(AiReviewGroupPage {
                groups: Vec::new(),
                total: 0,
            });
        };
        let grouped = format!(
            "SELECT a.key_kind, a.key, MIN(a.app_name), cat.id, cat.name, cat.color,
                    COUNT(DISTINCT a.id), {SECONDS}, MIN(a.ai_confidence),
                    GROUP_CONCAT(DISTINCT a.id)
             FROM activities a
             JOIN categories cat ON cat.id = a.ai_category_id
             LEFT JOIN sessions s ON s.activity_id = a.id AND s.end_time IS NOT NULL
             WHERE {pending}
             GROUP BY a.key_kind, a.key, cat.id
             HAVING {SECONDS} >= :min"
        );
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM ({grouped})"),
            named_params! {
                ":uncertain": filter.uncertain_only,
                ":confident": CONFIDENT, ":min": filter.min_seconds,
            },
            |row| row.get(0),
        )?;
        let mut stmt = conn.prepare(&format!(
            "{grouped} ORDER BY 8 DESC, a.key_kind, a.key LIMIT :limit OFFSET :offset"
        ))?;
        let groups = stmt
            .query_map(
                named_params! {
                    ":uncertain": filter.uncertain_only,
                    ":confident": CONFIDENT, ":min": filter.min_seconds,
                    ":limit": page_size, ":offset": page * page_size,
                },
                |row| {
                    let ids: String = row.get(9)?;
                    Ok(AiReviewGroup {
                        key_kind: row.get(0)?,
                        key: row.get(1)?,
                        app_name: row.get(2)?,
                        category_id: row.get(3)?,
                        category_name: row.get(4)?,
                        category_color: row.get(5)?,
                        activity_count: row.get(6)?,
                        total_seconds: row.get(7)?,
                        min_confidence: row.get(8)?,
                        activity_ids: ids.split(',').filter_map(|id| id.parse().ok()).collect(),
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(AiReviewGroupPage { groups, total })
    }

    pub fn feedback_stats(&self) -> Result<FeedbackStats> {
        let conn = self.conn.lock().unwrap();
        let mut stats = conn.query_row(
            &format!(
                "SELECT
                   COALESCE(SUM(category_id IS NOT NULL AND (
                       ai_category_id != category_id
                       OR (ai_category_id IS NULL AND ai_rejected_category_id IS NOT NULL))), 0),
                   COALESCE(SUM(category_id IS NOT NULL AND ai_category_id = category_id), 0),
                   COALESCE(SUM(category_id IS NOT NULL AND ai_category_id IS NULL
                                AND ai_rejected_category_id IS NULL), 0),
                   COALESCE(SUM(category_id IS NULL AND ai_rejected_category_id IS NOT NULL), 0),
                   COALESCE(SUM(category_id IS NULL AND ai_category_id IS NOT NULL
                                AND {NO_SESSION_PIN}), 0)
                 FROM activities a"
            ),
            [],
            |row| {
                Ok(FeedbackStats {
                    corrected: row.get(0)?,
                    confirmed: row.get(1)?,
                    pinned: row.get(2)?,
                    rejected: row.get(3)?,
                    pending_review: row.get(4)?,
                    ..FeedbackStats::default()
                })
            },
        )?;
        // Time, not rows: a few long activities matter more than many short ones.
        let (pending, reviewed) = conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(CASE WHEN pending THEN secs END), 0.0),
                        COALESCE(SUM(CASE WHEN reviewed THEN secs END), 0.0)
                 FROM (
                   SELECT (a.category_id IS NULL AND a.ai_category_id IS NOT NULL
                           AND {NO_SESSION_PIN}) AS pending,
                          ((a.ai_category_id IS NOT NULL AND a.category_id IS NOT NULL)
                           OR a.ai_rejected_category_id IS NOT NULL) AS reviewed,
                          (SELECT COALESCE(SUM((julianday(s.end_time) - julianday(s.start_time))
                                               * 86400.0), 0.0)
                           FROM sessions s
                           WHERE s.activity_id = a.id AND s.end_time IS NOT NULL) AS secs
                   FROM activities a)"
            ),
            [],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
        )?;
        stats.pending_seconds = pending;
        stats.reviewed_seconds = reviewed;
        Ok(stats)
    }
}
