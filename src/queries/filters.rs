use std::collections::HashSet;

use anyhow::Result;
use sqlx::Row;

use crate::db::sql;
use crate::filter::rules::{FilterAction, FilterField, FilterOperator};

use super::types::RawFilterRule;

// --- Read queries ---

pub async fn list_message_filters(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT id, pattern FROM message_filters WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn list_environment_filters(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT id, environment FROM environment_filters WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn list_release_filters(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT id, pattern FROM release_filters WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn list_user_agent_filters(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT id, pattern FROM user_agent_filters WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn list_filter_rules(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<RawFilterRule>> {
    let rows = sqlx::query(sql!(
        "SELECT id, field, operator, value, action, sample_rate, priority
         FROM filter_rules WHERE project_id = ?1 AND enabled = TRUE
         ORDER BY priority DESC, id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| RawFilterRule {
            id: r.get::<i64, _>(0),
            field: r.get::<String, _>(1),
            operator: r.get::<String, _>(2),
            value: r.get::<String, _>(3),
            action: r.get::<String, _>(4),
            sample_rate: r.get::<Option<f64>, _>(5),
            priority: r.get::<i32, _>(6),
        })
        .collect())
}

pub async fn list_ip_blocks(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(i64, String)>> {
    let rows = sqlx::query(sql!(
        "SELECT id, cidr FROM ip_blocklist WHERE project_id = ?1 ORDER BY id"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get::<i64, _>(0), r.get::<String, _>(1)))
        .collect())
}

/// Which inbound filters are turned on for a project.
pub async fn get_inbound_filters(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<HashSet<String>> {
    let rows = sqlx::query(sql!(
        "SELECT filter_id FROM inbound_filters WHERE project_id = ?1 AND enabled = TRUE"
    ))
    .bind(project_id as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get::<String, _>(0)).collect())
}

/// Project-level rate limit, or 0 if none is configured.
pub async fn get_rate_limit(pool: &crate::db::DbPool, project_id: u64) -> Result<u32> {
    let row = sqlx::query(
        sql!("SELECT max_events_per_minute FROM rate_limits WHERE project_id = ?1 AND public_key IS NULL"),
    )
    .bind(project_id as i64)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<i64, _>(0) as u32).unwrap_or(0))
}

/// Check if a fingerprint has been explicitly discarded.
pub async fn is_fingerprint_discarded(pool: &crate::db::DbPool, fingerprint: &str) -> Result<bool> {
    let row = sqlx::query(sql!(
        "SELECT COUNT(*) FROM discarded_fingerprints WHERE fingerprint = ?1"
    ))
    .bind(fingerprint)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<i64, _>(0) > 0).unwrap_or(false))
}

/// Discard stats from the last 7 days, grouped by date and reason.
pub async fn list_discard_stats(
    pool: &crate::db::DbPool,
    project_id: u64,
) -> Result<Vec<(String, String, u64)>> {
    #[cfg(feature = "sqlite")]
    let query = sql!(
        "SELECT date, reason, SUM(count) FROM discard_stats
         WHERE project_id = ?1 AND date >= date('now', '-7 days')
         GROUP BY date, reason ORDER BY date DESC, reason"
    );
    #[cfg(not(feature = "sqlite"))]
    let query = sql!(
        "SELECT date, reason, SUM(count) FROM discard_stats
         WHERE project_id = ?1 AND date >= (CURRENT_DATE - INTERVAL '7 days')::text
         GROUP BY date, reason ORDER BY date DESC, reason"
    );

    let rows = sqlx::query(query)
        .bind(project_id as i64)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>(0),
                r.get::<String, _>(1),
                r.get::<i64, _>(2) as u64,
            )
        })
        .collect())
}

// --- Write operations ---

pub async fn discard_fingerprint(
    pool: &crate::db::DbPool,
    fingerprint: &str,
    project_id: u64,
) -> Result<()> {
    #[cfg(feature = "sqlite")]
    let query = sql!(
        "INSERT OR IGNORE INTO discarded_fingerprints (fingerprint, project_id) VALUES (?1, ?2)"
    );
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO discarded_fingerprints (fingerprint, project_id) VALUES (?1, ?2) ON CONFLICT (fingerprint) DO NOTHING");

    sqlx::query(query)
        .bind(fingerprint)
        .bind(project_id as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn undiscard_fingerprint(pool: &crate::db::DbPool, fingerprint: &str) -> Result<()> {
    sqlx::query(sql!(
        "DELETE FROM discarded_fingerprints WHERE fingerprint = ?1"
    ))
    .bind(fingerprint)
    .execute(pool)
    .await?;
    Ok(())
}

// -- Inbound filters -------------------------------------------------------

pub async fn set_inbound_filter(
    pool: &crate::db::DbPool,
    project_id: u64,
    filter_id: &str,
    enabled: bool,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO inbound_filters (project_id, filter_id, enabled) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, filter_id) DO UPDATE SET enabled = excluded.enabled"
    ))
    .bind(project_id as i64)
    .bind(filter_id)
    .bind(enabled)
    .execute(pool)
    .await?;
    Ok(())
}

// -- Message filters -------------------------------------------------------

pub async fn create_message_filter(
    pool: &crate::db::DbPool,
    project_id: u64,
    pattern: &str,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO message_filters (project_id, pattern) VALUES (?1, ?2)"
    ))
    .bind(project_id as i64)
    .bind(pattern)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns 0 if the filter wasn't found.
pub async fn delete_message_filter(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM message_filters WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- Rate limits -----------------------------------------------------------

pub async fn set_rate_limit(
    pool: &crate::db::DbPool,
    project_id: u64,
    public_key: Option<&str>,
    max_events_per_minute: u32,
) -> Result<()> {
    sqlx::query(
        sql!("INSERT INTO rate_limits (project_id, public_key, max_events_per_minute) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, public_key) DO UPDATE SET max_events_per_minute = excluded.max_events_per_minute"),
    )
    .bind(project_id as i64)
    .bind(public_key)
    .bind(max_events_per_minute as i64)
    .execute(pool)
    .await?;
    Ok(())
}

// -- Environment filters ---------------------------------------------------

pub async fn add_environment_filter(
    pool: &crate::db::DbPool,
    project_id: u64,
    environment: &str,
) -> Result<()> {
    #[cfg(feature = "sqlite")]
    let query =
        sql!("INSERT OR IGNORE INTO environment_filters (project_id, environment) VALUES (?1, ?2)");
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO environment_filters (project_id, environment) VALUES (?1, ?2) ON CONFLICT (project_id, environment) DO NOTHING");

    sqlx::query(query)
        .bind(project_id as i64)
        .bind(environment)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns 0 if not found.
pub async fn delete_environment_filter(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM environment_filters WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- Release filters -------------------------------------------------------

pub async fn add_release_filter(
    pool: &crate::db::DbPool,
    project_id: u64,
    pattern: &str,
) -> Result<()> {
    #[cfg(feature = "sqlite")]
    let query = sql!("INSERT OR IGNORE INTO release_filters (project_id, pattern) VALUES (?1, ?2)");
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO release_filters (project_id, pattern) VALUES (?1, ?2) ON CONFLICT (project_id, pattern) DO NOTHING");

    sqlx::query(query)
        .bind(project_id as i64)
        .bind(pattern)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns 0 if not found.
pub async fn delete_release_filter(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM release_filters WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- User-agent filters ----------------------------------------------------

pub async fn add_user_agent_filter(
    pool: &crate::db::DbPool,
    project_id: u64,
    pattern: &str,
) -> Result<()> {
    sqlx::query(sql!(
        "INSERT INTO user_agent_filters (project_id, pattern) VALUES (?1, ?2)"
    ))
    .bind(project_id as i64)
    .bind(pattern)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns 0 if not found.
pub async fn delete_user_agent_filter(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM user_agent_filters WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- Filter rules ----------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn create_filter_rule(
    pool: &crate::db::DbPool,
    project_id: u64,
    field: &str,
    operator: &str,
    value: &str,
    action: &str,
    sample_rate: Option<f64>,
    priority: i32,
) -> Result<()> {
    if !FilterField::is_valid(field) {
        anyhow::bail!("unknown filter field '{field}'");
    }
    if !FilterOperator::is_valid(operator) {
        anyhow::bail!("unknown filter operator '{operator}'");
    }
    if !FilterAction::is_valid(action) {
        anyhow::bail!("unknown filter action '{action}'");
    }
    sqlx::query(
        sql!("INSERT INTO filter_rules (project_id, field, operator, value, action, sample_rate, priority)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
    )
    .bind(project_id as i64)
    .bind(field)
    .bind(operator)
    .bind(value)
    .bind(action)
    .bind(sample_rate)
    .bind(priority)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns 0 if not found.
pub async fn delete_filter_rule(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM filter_rules WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- IP blocklist ----------------------------------------------------------

pub async fn add_ip_block(pool: &crate::db::DbPool, project_id: u64, cidr: &str) -> Result<()> {
    #[cfg(feature = "sqlite")]
    let query = sql!("INSERT OR IGNORE INTO ip_blocklist (project_id, cidr) VALUES (?1, ?2)");
    #[cfg(not(feature = "sqlite"))]
    let query = sql!("INSERT INTO ip_blocklist (project_id, cidr) VALUES (?1, ?2) ON CONFLICT (project_id, cidr) DO NOTHING");

    sqlx::query(query)
        .bind(project_id as i64)
        .bind(cidr)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns 0 if not found.
pub async fn delete_ip_block(pool: &crate::db::DbPool, id: i64) -> Result<u64> {
    let result = sqlx::query(sql!("DELETE FROM ip_blocklist WHERE id = ?1"))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// -- Bulk filter data loading (used by FilterEngine) -----------------------

/// Load the full filter dataset from the database, parsed into domain types.
/// This feeds the FilterEngine -- one query per filter tier.
pub async fn load_filter_data(pool: &crate::db::DbPool) -> Result<crate::filter::FilterData> {
    use crate::filter::cidr::CidrBlock;
    use crate::filter::rules::{FilterAction, FilterField, FilterOperator, FilterRule};

    let mut data = crate::filter::FilterData::default();

    // Tier 1: discarded fingerprints -- cheapest check, bail early
    {
        let rows = sqlx::query(sql!("SELECT fingerprint FROM discarded_fingerprints"))
            .fetch_all(pool)
            .await?;
        for row in &rows {
            data.discarded.insert(row.get::<String, _>(0));
        }
    }

    // Tier 1: inbound filters
    {
        let rows = sqlx::query(sql!(
            "SELECT project_id, filter_id FROM inbound_filters WHERE enabled = TRUE"
        ))
        .fetch_all(pool)
        .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let fid = row.get::<String, _>(1);
            data.inbound_filters.entry(pid).or_default().insert(fid);
        }
    }

    // Tier 1: message filters -- pre-lowercased so glob matching doesn't allocate
    {
        let rows = sqlx::query(sql!("SELECT project_id, pattern FROM message_filters"))
            .fetch_all(pool)
            .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let pat = row.get::<String, _>(1);
            data.message_filters
                .entry(pid)
                .or_default()
                .push(pat.to_lowercase());
        }
    }

    // Tier 2: rate limits
    {
        let rows = sqlx::query(
            sql!("SELECT project_id, public_key, max_events_per_minute FROM rate_limits WHERE max_events_per_minute > 0"),
        )
        .fetch_all(pool)
        .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let pkey = row.get::<Option<String>, _>(1);
            let limit = row.get::<i64, _>(2) as u32;
            let key = match pkey {
                Some(k) if !k.is_empty() => format!("key:{k}"),
                _ => format!("project:{pid}"),
            };
            data.rate_limits.insert(key, limit);
        }
    }

    // Tier 2: excluded environments
    {
        let rows = sqlx::query(sql!(
            "SELECT project_id, environment FROM environment_filters"
        ))
        .fetch_all(pool)
        .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let env = row.get::<String, _>(1);
            data.excluded_environments
                .entry(pid)
                .or_default()
                .insert(env);
        }
    }

    // Tier 2: release filters -- pre-lowercased, same reason as message filters
    {
        let rows = sqlx::query(sql!("SELECT project_id, pattern FROM release_filters"))
            .fetch_all(pool)
            .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let pat = row.get::<String, _>(1);
            data.release_filters
                .entry(pid)
                .or_default()
                .push(pat.to_lowercase());
        }
    }

    // Tier 2: user-agent filters -- also pre-lowercased
    {
        let rows = sqlx::query(sql!("SELECT project_id, pattern FROM user_agent_filters"))
            .fetch_all(pool)
            .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let pat = row.get::<String, _>(1);
            data.ua_filters
                .entry(pid)
                .or_default()
                .push(pat.to_lowercase());
        }
    }

    // Tier 3: filter rules -- parsed into proper domain types
    {
        let rows = sqlx::query(sql!(
            "SELECT id, project_id, field, operator, value, action, sample_rate, priority
             FROM filter_rules WHERE enabled = TRUE ORDER BY priority DESC"
        ))
        .fetch_all(pool)
        .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(1) as u64;
            let field = row.get::<String, _>(2);
            let operator = row.get::<String, _>(3);
            let value = row.get::<String, _>(4);
            let action = row.get::<String, _>(5);
            let sample_rate = row.get::<Option<f64>, _>(6);

            let parsed_field = match FilterField::parse(&field) {
                Some(f) => f,
                None => {
                    tracing::error!(
                        "ignoring filter rule with unknown field '{field}' in project {pid}"
                    );
                    continue;
                }
            };
            let parsed_op = match FilterOperator::parse(&operator) {
                Some(o) => o,
                None => {
                    tracing::error!(
                        "ignoring filter rule with unknown operator '{operator}' in project {pid}"
                    );
                    continue;
                }
            };
            let parsed_action = match FilterAction::parse(&action) {
                Some(a) => a,
                None => {
                    tracing::error!(
                        "ignoring filter rule with unknown action '{action}' in project {pid}"
                    );
                    continue;
                }
            };
            let rule = FilterRule {
                field: parsed_field,
                operator: parsed_op,
                value,
                action: parsed_action,
                sample_rate,
            };
            data.filter_rules.entry(pid).or_default().push(rule);
        }
    }

    // Tier 3: IP blocklist -- parse CIDR strings, skip invalid ones
    {
        let rows = sqlx::query(sql!("SELECT project_id, cidr FROM ip_blocklist"))
            .fetch_all(pool)
            .await?;
        for row in &rows {
            let pid = row.get::<i64, _>(0) as u64;
            let cidr_str = row.get::<String, _>(1);
            match CidrBlock::parse(&cidr_str) {
                Some(block) => data.ip_blocklist.entry(pid).or_default().push(block),
                None => tracing::warn!("ignoring invalid CIDR entry: {cidr_str}"),
            }
        }
    }

    Ok(data)
}

/// Bump the discard stats counter for a given reason + date.
pub async fn upsert_discard_stats(
    pool: &crate::db::DbPool,
    project_id: u64,
    reason: &str,
    rule_id: Option<i64>,
    date: &str,
    count: u64,
) -> anyhow::Result<()> {
    sqlx::query(sql!(
        "INSERT INTO discard_stats (project_id, reason, rule_id, date, count)
         VALUES (?1, ?2, COALESCE(?3, 0), ?4, ?5)
         ON CONFLICT(project_id, reason, rule_id, date)
         DO UPDATE SET count = discard_stats.count + excluded.count"
    ))
    .bind(project_id as i64)
    .bind(reason)
    .bind(rule_id)
    .bind(date)
    .bind(count as i64)
    .execute(pool)
    .await?;
    Ok(())
}
