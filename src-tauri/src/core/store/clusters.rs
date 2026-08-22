use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::categories::{bump_classifier_input_revision, ensure_exists};
use super::resolve;
use super::sqlite::SharedConnection;

#[derive(Debug, Clone, Serialize)]
pub struct ActivityCluster {
    pub id: i64,
    pub scope: String,
    pub parent_kind: Option<String>,
    pub parent_key: Option<String>,
    pub label: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub confidence: Option<f32>,
    pub lead: Option<f32>,
    pub cohesion: f32,
    pub member_count: i64,
    pub seconds: f64,
    pub model_version: i64,
    pub created_at: String,
    pub members: Vec<ActivityClusterMember>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityClusterMember {
    pub id: i64,
    pub key_kind: String,
    pub key: String,
    pub url_key: Option<String>,
    pub window_title: Option<String>,
    pub seconds: f64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClusterAssignment {
    pub rules_created: i64,
    pub sessions_pinned: i64,
}

pub fn list_activity_clusters(conn: &SharedConnection) -> Result<Vec<ActivityCluster>> {
    let conn = conn.lock().unwrap();
    list_activity_clusters_from_conn(&conn)
}

fn list_activity_clusters_from_conn(conn: &Connection) -> Result<Vec<ActivityCluster>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.scope, a.parent_kind, a.parent_key, a.label,
                a.category_id, c.name, a.confidence, a.lead, a.cohesion,
                a.member_count, a.seconds, a.model_version, a.created_at
         FROM activity_clusters a
         LEFT JOIN categories c ON c.id = a.category_id
         ORDER BY a.scope, a.seconds DESC, a.id",
    )?;
    let mut clusters = stmt
        .query_map([], |row| {
            Ok(ActivityCluster {
                id: row.get(0)?,
                scope: row.get(1)?,
                parent_kind: row.get(2)?,
                parent_key: row.get(3)?,
                label: row.get(4)?,
                category_id: row.get(5)?,
                category_name: row.get(6)?,
                confidence: row.get(7)?,
                lead: row.get(8)?,
                cohesion: row.get(9)?,
                member_count: row.get(10)?,
                seconds: row.get(11)?,
                model_version: row.get(12)?,
                created_at: row.get(13)?,
                members: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let positions = clusters
        .iter()
        .enumerate()
        .map(|(index, cluster)| (cluster.id, index))
        .collect::<HashMap<_, _>>();
    let mut members = conn.prepare(
        "SELECT id, cluster_id, key_kind, key, url_key, window_title, seconds, session_count
         FROM activity_cluster_members
         ORDER BY cluster_id, seconds DESC, id",
    )?;
    for row in members.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            ActivityClusterMember {
                id: row.get(0)?,
                key_kind: row.get(2)?,
                key: row.get(3)?,
                url_key: row.get(4)?,
                window_title: row.get(5)?,
                seconds: row.get(6)?,
                session_count: row.get(7)?,
            },
        ))
    })? {
        let (_, cluster_id, member) = row?;
        if let Some(index) = positions.get(&cluster_id) {
            clusters[*index].members.push(member);
        }
    }
    Ok(clusters)
}

pub fn assign_activity_cluster(
    conn: &SharedConnection,
    cluster_id: i64,
    category_id: i64,
    member_ids: &[i64],
) -> Result<ClusterAssignment> {
    let mut conn = conn.lock().unwrap();
    let tx = conn.transaction()?;
    ensure_exists(&tx, category_id)?;
    let scope: String = tx
        .query_row(
            "SELECT scope FROM activity_clusters WHERE id = ?1",
            params![cluster_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("activity cluster {cluster_id} is no longer available"))?;

    let mut selected_ids = member_ids.to_vec();
    if member_ids.is_empty() {
        let mut stmt = tx.prepare(
            "SELECT id FROM activity_cluster_members
             WHERE cluster_id = ?1 ORDER BY id",
        )?;
        selected_ids = stmt
            .query_map(params![cluster_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
    } else {
        let unique = member_ids.iter().copied().collect::<HashSet<_>>();
        if unique.len() != member_ids.len() {
            bail!("activity cluster assignment contains duplicate members");
        }
    }
    if selected_ids.is_empty() {
        bail!("activity cluster assignment must include at least one member");
    }

    let placeholders = std::iter::repeat_n("?", selected_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, key_kind, key, url_key, window_title
         FROM activity_cluster_members
         WHERE cluster_id = ?1 AND id IN ({placeholders})
         ORDER BY id"
    );
    let mut statement = tx.prepare(&sql)?;
    let mut values = Vec::with_capacity(selected_ids.len() + 1);
    values.push(rusqlite::types::Value::Integer(cluster_id));
    values.extend(
        selected_ids
            .iter()
            .map(|id| rusqlite::types::Value::Integer(*id)),
    );
    let members = statement
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok(ActivityClusterMember {
                id: row.get(0)?,
                key_kind: row.get(1)?,
                key: row.get(2)?,
                url_key: row.get(3)?,
                window_title: row.get(4)?,
                seconds: 0.0,
                session_count: 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if members.len() != selected_ids.len() {
        bail!("one or more activity cluster members are no longer available");
    }
    drop(statement);

    let mut outcome = ClusterAssignment::default();
    match scope.as_str() {
        "key" => {
            for member in &members {
                tx.execute(
                    "INSERT INTO category_rules (key_kind, pattern, match_kind, category_id)
                     VALUES (?1, ?2, 'exact', ?3)
                     ON CONFLICT(key_kind, match_kind, pattern) DO UPDATE SET
                        category_id = excluded.category_id",
                    params![member.key_kind, member.key, category_id],
                )?;
                tx.execute(
                    "DELETE FROM category_suggestions
                     WHERE status = 'pending' AND key_kind = ?1 AND key = ?2",
                    params![member.key_kind, member.key],
                )?;
                outcome.rules_created += 1;
            }
        }
        "title" => {
            for member in &members {
                let changed = tx.execute(
                    "UPDATE sessions
                     SET category_id = ?1,
                         ml_category_id = NULL,
                         ml_confidence = NULL,
                         ml_model_version = NULL
                     WHERE category_key_kind = ?2 AND category_key = ?3
                       AND url_key IS ?4
                       AND window_title IS ?5",
                    params![
                        category_id,
                        member.key_kind,
                        member.key,
                        member.url_key,
                        member.window_title,
                    ],
                )?;
                outcome.sessions_pinned += changed as i64;
            }
        }
        other => bail!("unknown activity cluster scope: {other}"),
    }
    tx.execute(
        "DELETE FROM activity_clusters WHERE id = ?1",
        params![cluster_id],
    )?;
    bump_classifier_input_revision(&tx)?;
    resolve::resolve_all(&tx)?;
    tx.commit()?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::store::categories::CategoryStore;
    use crate::core::store::sqlite::open_connection;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "makora-clusters-{name}-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn title_assignment_matches_a_null_url_key() {
        let path = temp_path("null-url");
        let conn = open_connection(&path).unwrap();
        let store = CategoryStore::new(conn.clone());
        let category = store.create_category("Learning", "#111111", None).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO sessions
                    (app_name, exe_path, window_title, category_key, category_key_kind,
                     start_time, end_time)
                 VALUES ('Editor', 'editor.exe', 'Rust course', 'editor.exe', 'exe',
                         '2024-01-01T00:00:00Z', '2024-01-01T00:10:00Z')",
                [],
            )
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_clusters
                    (scope, parent_kind, parent_key, label, cohesion, member_count,
                     seconds, model_version, created_at)
                 VALUES ('title', 'exe', 'editor.exe', 'rust course', 1.0, 1,
                         600.0, 1, '2024-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        let cluster_id = conn.lock().unwrap().last_insert_rowid();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_cluster_members
                    (cluster_id, key_kind, key, url_key, window_title, seconds)
                 VALUES (?1, 'exe', 'editor.exe', NULL, 'Rust course', 600.0)",
                params![cluster_id],
            )
            .unwrap();
        let member_id = conn.lock().unwrap().last_insert_rowid();

        let outcome = store
            .assign_activity_cluster(cluster_id, category.id, &[member_id])
            .unwrap();
        assert_eq!(outcome.sessions_pinned, 1);
        let pinned: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT category_id FROM sessions WHERE window_title = 'Rust course'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pinned, Some(category.id));
        assert!(list_activity_clusters(&conn).unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn key_assignment_mints_an_exact_rule_and_replays_history() {
        let path = temp_path("key");
        let conn = open_connection(&path).unwrap();
        let store = CategoryStore::new(conn.clone());
        let category = store
            .create_category("Development", "#111111", None)
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO sessions
                    (app_name, exe_path, window_title, category_key, category_key_kind,
                     start_time, end_time)
                 VALUES ('Editor', 'editor.exe', 'Code', 'editor.exe', 'exe',
                         '2024-01-01T00:00:00Z', '2024-01-01T00:10:00Z')",
                [],
            )
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_clusters
                    (scope, label, cohesion, member_count, seconds, model_version, created_at)
                 VALUES ('key', 'code', 1.0, 1, 600.0, 1, '2024-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        let cluster_id = conn.lock().unwrap().last_insert_rowid();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_cluster_members
                    (cluster_id, key_kind, key, seconds)
                 VALUES (?1, 'exe', 'editor.exe', 600.0)",
                params![cluster_id],
            )
            .unwrap();
        let member_id = conn.lock().unwrap().last_insert_rowid();

        let outcome = store
            .assign_activity_cluster(cluster_id, category.id, &[member_id])
            .unwrap();
        assert_eq!(outcome.rules_created, 1);
        assert!(store
            .list_rules()
            .unwrap()
            .iter()
            .any(|rule| rule.pattern == "editor.exe" && rule.category_id == category.id));
        let resolved: Option<i64> = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rule_category_id FROM sessions WHERE category_key = 'editor.exe'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolved, Some(category.id));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn assignment_rejects_a_member_from_another_cluster() {
        let path = temp_path("membership");
        let conn = open_connection(&path).unwrap();
        let store = CategoryStore::new(conn.clone());
        let category = store.create_category("Work", "#111111", None).unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_clusters
                    (scope, label, cohesion, member_count, seconds, model_version, created_at)
                 VALUES ('key', 'work', 1.0, 1, 1.0, 1, '2024-01-01T00:00:00Z'),
                        ('key', 'other', 1.0, 1, 1.0, 1, '2024-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        let ids = {
            let conn = conn.lock().unwrap();
            let first: i64 = conn
                .query_row(
                    "SELECT id FROM activity_clusters ORDER BY id LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let second: i64 = conn
                .query_row(
                    "SELECT id FROM activity_clusters ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            (first, second)
        };
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO activity_cluster_members (cluster_id, key_kind, key, seconds)
                 VALUES (?1, 'exe', 'one.exe', 1.0), (?2, 'exe', 'two.exe', 1.0)",
                params![ids.0, ids.1],
            )
            .unwrap();
        let second_member = conn.lock().unwrap().last_insert_rowid();
        assert!(store
            .assign_activity_cluster(ids.0, category.id, &[second_member])
            .is_err());
        assert_eq!(list_activity_clusters(&conn).unwrap().len(), 2);
        let _ = std::fs::remove_file(path);
    }
}
