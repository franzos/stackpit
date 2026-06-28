use crate::db::{sql, DbPool};
use sqlx::QueryBuilder;
use std::collections::{BTreeMap, HashMap};

/// Max fingerprints per IN-clause chunk. 1 bind param each;
/// SQLite limit is 32766, use 30000 for a comfortable margin.
pub(super) const TRIGGER_CHUNK_SIZE: usize = 30_000;

/// Data needed to check threshold alerts after the write transaction commits.
pub(super) struct ThresholdCandidate {
    pub(super) fingerprint: String,
    pub(super) project_id: u64,
    pub(super) title: Option<String>,
    pub(super) level: Option<String>,
}

/// Check threshold alert rules for existing issues -- batched.
///
/// Instead of per-candidate × per-rule queries (N+1), this:
/// 1. Fetches all threshold rules once (small, human-managed table)
/// 2. Matches rules to candidates in memory
/// 3. Batch-fetches cooldown state
/// 4. Batch-counts events per (fingerprint, window) group
/// 5. Updates only triggered alert states (rare)
pub(super) async fn check_threshold_alerts(
    pool: &DbPool,
    candidates: &[ThresholdCandidate],
    pending: &mut Vec<crate::notify::NotificationEvent>,
) {
    use sqlx::Row;

    if candidates.is_empty() {
        return;
    }

    let all_rules = match sqlx::query(sql!(
        "SELECT id, project_id, fingerprint, threshold_count, window_secs, cooldown_secs
         FROM alert_rules WHERE enabled = TRUE AND trigger_kind = 'threshold'"
    ))
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("threshold alert: failed to query rules: {e}");
            return;
        }
    };

    if all_rules.is_empty() {
        return;
    }

    struct Rule {
        id: i64,
        project_id: Option<i64>,
        fingerprint: Option<String>,
        threshold: i64,
        window: i64,
        cooldown: i64,
    }

    let rules: Vec<Rule> = all_rules
        .iter()
        .filter_map(|row| {
            Some(Rule {
                id: row.get("id"),
                project_id: row.get("project_id"),
                fingerprint: row.get("fingerprint"),
                threshold: row.get::<Option<i64>, _>("threshold_count")?,
                window: row.get::<Option<i64>, _>("window_secs")?,
                cooldown: row.get("cooldown_secs"),
            })
        })
        .collect();

    if rules.is_empty() {
        return;
    }

    struct RuleMatch {
        rule_idx: usize,
        candidate_idx: usize,
    }

    let mut matches: Vec<RuleMatch> = Vec::new();
    for (ci, c) in candidates.iter().enumerate() {
        for (ri, rule) in rules.iter().enumerate() {
            let project_ok = rule.project_id.is_none_or(|pid| pid == c.project_id as i64);
            let fp_ok = rule
                .fingerprint
                .as_deref()
                .is_none_or(|fp| fp == c.fingerprint);
            if project_ok && fp_ok {
                matches.push(RuleMatch {
                    rule_idx: ri,
                    candidate_idx: ci,
                });
            }
        }
    }

    if matches.is_empty() {
        return;
    }

    let unique_rule_ids: Vec<i64> = {
        let mut v: Vec<i64> = rules.iter().map(|r| r.id).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let unique_fps: Vec<&str> = {
        let mut v: Vec<&str> = candidates.iter().map(|c| c.fingerprint.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v
    };

    let mut cooldowns: HashMap<(i64, String), i64> = HashMap::new();
    for rid_chunk in unique_rule_ids.chunks(TRIGGER_CHUNK_SIZE) {
        for fp_chunk in unique_fps.chunks(TRIGGER_CHUNK_SIZE) {
            let mut qb = QueryBuilder::<crate::db::Db>::new(
                "SELECT alert_rule_id, fingerprint, last_triggered FROM alert_state WHERE alert_rule_id IN (",
            );
            {
                let mut sep = qb.separated(", ");
                for &rid in rid_chunk {
                    sep.push_bind(rid);
                }
            }
            qb.push(") AND fingerprint IN (");
            {
                let mut sep = qb.separated(", ");
                for fp in fp_chunk {
                    sep.push_bind(*fp);
                }
            }
            qb.push(")");

            if let Ok(rows) = qb.build().fetch_all(pool).await {
                for row in &rows {
                    cooldowns.insert(
                        (row.get("alert_rule_id"), row.get("fingerprint")),
                        row.get("last_triggered"),
                    );
                }
            }
        }
    }

    let now = chrono::Utc::now().timestamp();

    struct PendingCheck {
        rule_idx: usize,
        candidate_idx: usize,
    }

    let mut by_window: BTreeMap<i64, Vec<PendingCheck>> = BTreeMap::new();
    for m in &matches {
        let rule = &rules[m.rule_idx];
        let fp = &candidates[m.candidate_idx].fingerprint;
        if let Some(&last) = cooldowns.get(&(rule.id, fp.clone())) {
            if now - last < rule.cooldown {
                continue;
            }
        }
        by_window
            .entry(rule.window)
            .or_default()
            .push(PendingCheck {
                rule_idx: m.rule_idx,
                candidate_idx: m.candidate_idx,
            });
    }

    if by_window.is_empty() {
        return;
    }

    let mut event_counts: BTreeMap<(i64, String), i64> = BTreeMap::new();
    for (&window, checks) in &by_window {
        let mut fps: Vec<&str> = checks
            .iter()
            .map(|c| candidates[c.candidate_idx].fingerprint.as_str())
            .collect();
        fps.sort_unstable();
        fps.dedup();

        let since = now - window;
        for chunk in fps.chunks(TRIGGER_CHUNK_SIZE) {
            let mut qb = QueryBuilder::<crate::db::Db>::new(
                "SELECT fingerprint, COUNT(*) as cnt FROM events WHERE timestamp >= ",
            );
            qb.push_bind(since);
            qb.push(" AND fingerprint IN (");
            {
                let mut sep = qb.separated(", ");
                for fp in chunk {
                    sep.push_bind(*fp);
                }
            }
            qb.push(") GROUP BY fingerprint");

            if let Ok(rows) = qb.build().fetch_all(pool).await {
                for row in &rows {
                    event_counts.insert((window, row.get("fingerprint")), row.get("cnt"));
                }
            }
        }
    }

    for (&window, checks) in &by_window {
        for check in checks {
            let rule = &rules[check.rule_idx];
            let c = &candidates[check.candidate_idx];
            let count = event_counts
                .get(&(window, c.fingerprint.clone()))
                .copied()
                .unwrap_or(0);

            if count < rule.threshold {
                continue;
            }

            if let Err(e) = sqlx::query(sql!(
                "INSERT INTO alert_state (alert_rule_id, fingerprint, last_triggered)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(alert_rule_id, fingerprint) DO UPDATE SET last_triggered = excluded.last_triggered"
            ))
            .bind(rule.id)
            .bind(&c.fingerprint)
            .bind(now)
            .execute(pool)
            .await
            {
                tracing::warn!(
                    "threshold alert: failed to update cooldown state for rule {}: {e}",
                    rule.id
                );
            }

            pending.push(crate::notify::NotificationEvent {
                trigger: crate::notify::NotifyTrigger::ThresholdExceeded {
                    rule_id: rule.id,
                    count,
                    window_secs: window,
                },
                project_id: c.project_id,
                fingerprint: c.fingerprint.clone(),
                title: c.title.clone(),
                level: c.level.clone(),
                environment: None,
                event_id: String::new(),
                digest: None,
            });
        }
    }
}
