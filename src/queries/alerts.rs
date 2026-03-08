use std::collections::HashMap;

use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::db::DbPool;

// -- Alert rule types ---

pub struct AlertRule {
    pub id: i64,
    pub project_id: Option<u64>,
    pub fingerprint: Option<String>,
    pub trigger_kind: String,
    pub threshold_count: Option<i64>,
    pub window_secs: Option<i64>,
    pub cooldown_secs: i64,
    pub enabled: bool,
    pub created_at: i64,
}

fn map_alert_rule(row: &crate::db::DbRow) -> AlertRule {
    AlertRule {
        id: row.get("id"),
        project_id: row.get::<Option<i64>, _>("project_id").map(|v| v as u64),
        fingerprint: row.get("fingerprint"),
        trigger_kind: row.get("trigger_kind"),
        threshold_count: row.get("threshold_count"),
        window_secs: row.get("window_secs"),
        cooldown_secs: row.get("cooldown_secs"),
        enabled: row.get::<bool, _>("enabled"),
        created_at: row.get("created_at"),
    }
}

// -- Alert rule CRUD ---

pub async fn create_alert_rule(
    pool: &DbPool,
    project_id: Option<u64>,
    fingerprint: Option<&str>,
    trigger_kind: &str,
    threshold_count: Option<i64>,
    window_secs: Option<i64>,
    cooldown_secs: i64,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    {
        let result = sqlx::query(sql!(
            "INSERT INTO alert_rules (project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ))
        .bind(project_id.map(|v| v as i64))
        .bind(fingerprint)
        .bind(trigger_kind)
        .bind(threshold_count)
        .bind(window_secs)
        .bind(cooldown_secs)
        .execute(pool)
        .await?;
        Ok(result.last_insert_rowid())
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let row = sqlx::query(sql!(
            "INSERT INTO alert_rules (project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id"
        ))
        .bind(project_id.map(|v| v as i64))
        .bind(fingerprint)
        .bind(trigger_kind)
        .bind(threshold_count)
        .bind(window_secs)
        .bind(cooldown_secs)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }
}

pub async fn update_alert_rule(
    pool: &DbPool,
    id: i64,
    threshold_count: Option<i64>,
    window_secs: Option<i64>,
    cooldown_secs: i64,
    enabled: bool,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE alert_rules SET threshold_count = ?1, window_secs = ?2, cooldown_secs = ?3, enabled = ?4 WHERE id = ?5"
    ))
    .bind(threshold_count)
    .bind(window_secs)
    .bind(cooldown_secs)
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn delete_alert_rule(pool: &DbPool, id: i64) -> Result<u64> {
    let mut tx = pool.begin().await?;
    // Clean up cooldown state first
    sqlx::query(sql!("DELETE FROM alert_state WHERE alert_rule_id = ?1"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query(sql!("DELETE FROM alert_rules WHERE id = ?1"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

pub async fn list_alert_rules(pool: &DbPool, project_id: Option<u64>) -> Result<Vec<AlertRule>> {
    let rows = match project_id {
        Some(pid) => {
            sqlx::query(sql!(
                "SELECT id, project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs, enabled, created_at
                 FROM alert_rules WHERE project_id = ?1 OR project_id IS NULL ORDER BY id"
            ))
            .bind(pid as i64)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(sql!(
                "SELECT id, project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs, enabled, created_at
                 FROM alert_rules ORDER BY id"
            ))
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.iter().map(map_alert_rule).collect())
}

#[allow(dead_code)]
pub async fn get_alert_rule(pool: &DbPool, id: i64) -> Result<Option<AlertRule>> {
    let row = sqlx::query(sql!(
        "SELECT id, project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs, enabled, created_at
         FROM alert_rules WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(map_alert_rule))
}

// -- Threshold helpers (test-only; production uses batched queries in writer::flush) --

#[cfg(test)]
async fn matching_threshold_rules(
    pool: &DbPool,
    project_id: u64,
    fingerprint: &str,
) -> Result<Vec<AlertRule>> {
    let rows = sqlx::query(sql!(
        "SELECT id, project_id, fingerprint, trigger_kind, threshold_count, window_secs, cooldown_secs, enabled, created_at
         FROM alert_rules
         WHERE enabled = TRUE
           AND trigger_kind = 'threshold'
           AND (project_id IS NULL OR project_id = ?1)
           AND (fingerprint IS NULL OR fingerprint = ?2)"
    ))
    .bind(project_id as i64)
    .bind(fingerprint)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(map_alert_rule).collect())
}

#[cfg(test)]
async fn is_in_cooldown(
    pool: &DbPool,
    rule_id: i64,
    fingerprint: &str,
    now: i64,
    cooldown_secs: i64,
) -> Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT last_triggered FROM alert_state WHERE alert_rule_id = ?1 AND fingerprint = ?2"
    ))
    .bind(rule_id)
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => {
            let ts: i64 = r.get("last_triggered");
            Ok(now - ts < cooldown_secs)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
async fn record_trigger(pool: &DbPool, rule_id: i64, fingerprint: &str, now: i64) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO alert_state (alert_rule_id, fingerprint, last_triggered)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(alert_rule_id, fingerprint) DO UPDATE SET last_triggered = excluded.last_triggered"
    ))
    .bind(rule_id)
    .bind(fingerprint)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

// -- Digest schedule CRUD ---

pub struct DigestSchedule {
    pub id: i64,
    pub project_id: Option<u64>,
    pub interval_secs: i64,
    pub last_sent: i64,
    pub enabled: bool,
    pub created_at: i64,
}

fn map_digest_schedule(row: &crate::db::DbRow) -> DigestSchedule {
    DigestSchedule {
        id: row.get("id"),
        project_id: row.get::<Option<i64>, _>("project_id").map(|v| v as u64),
        interval_secs: row.get("interval_secs"),
        last_sent: row.get("last_sent"),
        enabled: row.get::<bool, _>("enabled"),
        created_at: row.get("created_at"),
    }
}

pub async fn create_digest_schedule(
    pool: &DbPool,
    project_id: Option<u64>,
    interval_secs: i64,
) -> Result<i64> {
    #[cfg(feature = "sqlite")]
    {
        let result = sqlx::query(sql!(
            "INSERT INTO digest_schedules (project_id, interval_secs) VALUES (?1, ?2)"
        ))
        .bind(project_id.map(|v| v as i64))
        .bind(interval_secs)
        .execute(pool)
        .await?;
        Ok(result.last_insert_rowid())
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let row = sqlx::query(sql!(
            "INSERT INTO digest_schedules (project_id, interval_secs) VALUES (?1, ?2) RETURNING id"
        ))
        .bind(project_id.map(|v| v as i64))
        .bind(interval_secs)
        .fetch_one(pool)
        .await?;
        Ok(row.get::<i64, _>("id"))
    }
}

pub async fn update_digest_schedule(
    pool: &DbPool,
    id: i64,
    interval_secs: i64,
    enabled: bool,
) -> Result<u64> {
    let result = sqlx::query(sql!(
        "UPDATE digest_schedules SET interval_secs = ?1, enabled = ?2 WHERE id = ?3"
    ))
    .bind(interval_secs)
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn delete_digest_schedule(pool: &DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM digest_schedules WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn list_digest_schedules(pool: &DbPool) -> Result<Vec<DigestSchedule>> {
    let rows = sqlx::query(sql!(
        "SELECT id, project_id, interval_secs, last_sent, enabled, created_at
         FROM digest_schedules ORDER BY id"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(map_digest_schedule).collect())
}

#[allow(dead_code)]
pub async fn get_digest_schedule(pool: &DbPool, id: i64) -> Result<Option<DigestSchedule>> {
    let row = sqlx::query(sql!(
        "SELECT id, project_id, interval_secs, last_sent, enabled, created_at
         FROM digest_schedules WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(map_digest_schedule))
}

// -- Digest task queries ---

/// Digest schedules that are due -- i.e. enough time has passed since last send.
pub async fn list_due_digests(pool: &DbPool, now: i64) -> Result<Vec<DigestSchedule>> {
    let rows = sqlx::query(sql!(
        "SELECT id, project_id, interval_secs, last_sent, enabled, created_at
         FROM digest_schedules
         WHERE enabled = TRUE AND (last_sent + interval_secs) <= ?1"
    ))
    .bind(now)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(map_digest_schedule).collect())
}

/// Stamp a digest schedule as sent right now.
pub async fn update_digest_last_sent(pool: &DbPool, id: i64, now: i64) -> Result<()> {
    sqlx::query(sql!(
        "UPDATE digest_schedules SET last_sent = ?1 WHERE id = ?2"
    ))
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Build the digest payload for a time range -- new issues, active counts, totals.
pub async fn build_digest_data(
    pool: &DbPool,
    period_start: i64,
    period_end: i64,
    project_id: Option<u64>,
) -> Result<Vec<crate::notify::DigestProject>> {
    use crate::notify::{DigestIssue, DigestProject};

    // Figure out which projects we're covering
    let projects: Vec<(u64, Option<String>)> = match project_id {
        Some(pid) => {
            let row = sqlx::query(sql!("SELECT name FROM projects WHERE project_id = ?1"))
                .bind(pid as i64)
                .fetch_optional(pool)
                .await?;
            let name: Option<String> = row.map(|r| r.get("name"));
            vec![(pid, name)]
        }
        None => {
            let rows = sqlx::query(sql!(
                "SELECT project_id, name FROM projects WHERE status = 'active'"
            ))
            .fetch_all(pool)
            .await?;
            rows.iter()
                .map(|r| (r.get::<i64, _>("project_id") as u64, r.get("name")))
                .collect()
        }
    };

    // Collect per-project event stats -- batch in chunks to stay within SQLite's variable limit
    let project_ids: Vec<u64> = projects.iter().map(|(pid, _)| *pid).collect();
    let mut stats_map: HashMap<u64, (u64, u64)> = HashMap::new();
    for chunk in project_ids.chunks(500) {
        let mut qb = sqlx::QueryBuilder::<crate::db::Db>::new(
            "SELECT project_id, COUNT(DISTINCT fingerprint), COUNT(*)
             FROM events
             WHERE project_id IN (",
        );
        let mut sep = qb.separated(", ");
        for pid in chunk {
            sep.push_bind(*pid as i64);
        }
        qb.push(") AND timestamp >= ");
        qb.push_bind(period_start);
        qb.push(" AND timestamp < ");
        qb.push_bind(period_end);
        qb.push(" GROUP BY project_id");

        let rows = qb.build().fetch_all(pool).await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let active = row.get::<i64, _>(1) as u64;
            let total = row.get::<i64, _>(2) as u64;
            stats_map.insert(pid, (active, total));
        }
    }

    let mut result = Vec::new();

    for (pid, name) in projects {
        let (active_issues_count, total_events) = stats_map.get(&pid).copied().unwrap_or((0, 0));

        // Issues that first appeared during this period
        let new_issue_rows = sqlx::query(sql!(
            "SELECT fingerprint, title, level, event_count, first_seen
             FROM issues
             WHERE project_id = ?1 AND first_seen >= ?2 AND first_seen < ?3
             ORDER BY event_count DESC
             LIMIT 50"
        ))
        .bind(pid as i64)
        .bind(period_start)
        .bind(period_end)
        .fetch_all(pool)
        .await?;

        let new_issues: Vec<DigestIssue> = new_issue_rows
            .iter()
            .map(|r| DigestIssue {
                fingerprint: r.get("fingerprint"),
                title: r.get("title"),
                level: r.get("level"),
                event_count: r.get::<i64, _>("event_count") as u64,
                first_seen: r.get("first_seen"),
            })
            .collect();

        // Skip projects with zero activity -- no point cluttering the digest
        if total_events > 0 || !new_issues.is_empty() {
            result.push(DigestProject {
                project_id: pid,
                name,
                new_issues,
                active_issues_count,
                total_events,
            });
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::test_helpers::*;

    #[tokio::test]
    async fn alert_rule_crud() {
        let pool = open_test_db().await;

        let id = create_alert_rule(
            &pool,
            Some(1),
            None,
            "threshold",
            Some(100),
            Some(3600),
            3600,
        )
        .await
        .unwrap();
        assert!(id > 0);

        let rule = get_alert_rule(&pool, id).await.unwrap().unwrap();
        assert_eq!(rule.threshold_count, Some(100));
        assert_eq!(rule.window_secs, Some(3600));
        assert!(rule.enabled);

        update_alert_rule(&pool, id, Some(50), Some(1800), 7200, false)
            .await
            .unwrap();
        let rule = get_alert_rule(&pool, id).await.unwrap().unwrap();
        assert_eq!(rule.threshold_count, Some(50));
        assert!(!rule.enabled);

        delete_alert_rule(&pool, id).await.unwrap();
        assert!(get_alert_rule(&pool, id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn threshold_rule_matching() {
        let pool = open_test_db().await;

        // Global rule (project_id = NULL)
        let global = create_alert_rule(&pool, None, None, "threshold", Some(10), Some(3600), 3600)
            .await
            .unwrap();
        // Project-specific rule
        let specific =
            create_alert_rule(&pool, Some(1), None, "threshold", Some(5), Some(1800), 3600)
                .await
                .unwrap();
        // Rule for a different project
        create_alert_rule(
            &pool,
            Some(2),
            None,
            "threshold",
            Some(20),
            Some(3600),
            3600,
        )
        .await
        .unwrap();

        let rules = matching_threshold_rules(&pool, 1, "fp-abc").await.unwrap();
        let ids: Vec<i64> = rules.iter().map(|r| r.id).collect();
        assert!(ids.contains(&global));
        assert!(ids.contains(&specific));
        assert_eq!(ids.len(), 2);
    }

    #[tokio::test]
    async fn cooldown_tracking() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        let rule_id = create_alert_rule(
            &pool,
            Some(1),
            None,
            "threshold",
            Some(10),
            Some(3600),
            3600,
        )
        .await
        .unwrap();

        // Not in cooldown initially
        assert!(!is_in_cooldown(&pool, rule_id, "fp-1", now, 3600)
            .await
            .unwrap());

        // Record trigger
        record_trigger(&pool, rule_id, "fp-1", now).await.unwrap();

        // Now in cooldown
        assert!(is_in_cooldown(&pool, rule_id, "fp-1", now + 100, 3600)
            .await
            .unwrap());

        // Past cooldown
        assert!(!is_in_cooldown(&pool, rule_id, "fp-1", now + 3601, 3600)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn digest_schedule_crud() {
        let pool = open_test_db().await;

        let id = create_digest_schedule(&pool, Some(1), 86400).await.unwrap();
        assert!(id > 0);

        let schedule = get_digest_schedule(&pool, id).await.unwrap().unwrap();
        assert_eq!(schedule.interval_secs, 86400);
        assert!(schedule.enabled);

        update_digest_schedule(&pool, id, 604800, false)
            .await
            .unwrap();
        let schedule = get_digest_schedule(&pool, id).await.unwrap().unwrap();
        assert_eq!(schedule.interval_secs, 604800);
        assert!(!schedule.enabled);

        delete_digest_schedule(&pool, id).await.unwrap();
        assert!(get_digest_schedule(&pool, id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn due_digests() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        // Schedule that's due (last_sent = 0, interval = 3600)
        let id = create_digest_schedule(&pool, None, 3600).await.unwrap();

        let due = list_due_digests(&pool, now).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id);

        // Mark as sent
        update_digest_last_sent(&pool, id, now).await.unwrap();

        // Not due anymore
        let due = list_due_digests(&pool, now + 100).await.unwrap();
        assert!(due.is_empty());

        // Due again after interval
        let due = list_due_digests(&pool, now + 3601).await.unwrap();
        assert_eq!(due.len(), 1);
    }

    #[tokio::test]
    async fn digest_data_with_events() {
        let pool = open_test_db().await;
        let now = chrono::Utc::now().timestamp();

        // Set up project
        sqlx::query("INSERT INTO projects (project_id, name, status) VALUES (1, 'Test', 'active')")
            .execute(&pool)
            .await
            .unwrap();

        // Insert events and issues
        insert_test_event(
            &pool,
            "e1",
            1,
            now - 100,
            Some("fp-1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        insert_test_event(
            &pool,
            "e2",
            1,
            now - 50,
            Some("fp-1"),
            Some("error"),
            Some("Error A"),
        )
        .await;
        insert_test_issue(
            &pool,
            "fp-1",
            1,
            Some("Error A"),
            Some("error"),
            now - 100,
            now - 50,
            2,
            "unresolved",
        )
        .await;

        let data = build_digest_data(&pool, now - 200, now, None)
            .await
            .unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].project_id, 1);
        assert_eq!(data[0].total_events, 2);
        assert_eq!(data[0].new_issues.len(), 1);
        assert_eq!(data[0].new_issues[0].fingerprint, "fp-1");
    }
}
