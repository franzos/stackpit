//! Persistence for the activated license. Singleton row `stackpit_license`
//! (PK pinned to 1). Kept inside `src/commercial/` so all license code stays
//! under LICENSE-COMMERCIAL. Uses the SQLx `sql!` macro for sqlite/postgres
//! placeholder portability.

use chrono::Utc;

use crate::commercial::license::{classify, License, LicenseStatus};
use crate::commercial::verify;
use crate::db::{sql, DbPool};

/// Read + verify the persisted license at boot. Empty table -> Unlicensed.
/// A previously-accepted blob that no longer verifies (e.g. key rotated)
/// logs a warning and falls back to Unlicensed so the app still boots.
pub async fn load(pool: &DbPool, grace_days: i64) -> LicenseStatus {
    let status = match read_blob(pool).await {
        Ok(None) => LicenseStatus::Unlicensed,
        Ok(Some(blob)) => match verify::decode_and_verify(&blob) {
            Ok(license) => classify(license, grace_days, Utc::now()),
            Err(e) => {
                tracing::warn!(error = %e, "license: persisted blob no longer verifies; operator must re-activate");
                LicenseStatus::Unlicensed
            }
        },
        Err(e) => {
            tracing::error!(error = ?e, "license: failed to read row at boot; treating as unlicensed");
            LicenseStatus::Unlicensed
        }
    };
    log_status(&status);
    status
}

fn log_status(status: &LicenseStatus) {
    match status {
        LicenseStatus::Unlicensed => tracing::info!("license: unlicensed (OSS tier)"),
        LicenseStatus::Active(l) => {
            tracing::info!(customer = %l.customer, email = %l.email, expires_at = ?l.expires_at, "license: active")
        }
        LicenseStatus::Grace(l) => {
            tracing::warn!(customer = %l.customer, expires_at = ?l.expires_at, "license: in grace period")
        }
        LicenseStatus::Expired(l) => {
            tracing::warn!(customer = %l.customer, expires_at = ?l.expires_at, "license: expired past grace window")
        }
    }
}

async fn read_blob(pool: &DbPool) -> anyhow::Result<Option<String>> {
    use sqlx::Row;
    let row = sqlx::query(sql!("SELECT blob FROM stackpit_license WHERE id = 1"))
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("blob")))
}

/// Upsert the singleton row from a freshly-verified license. `write_pool`
/// must be a writer pool (the read pool is query_only on sqlite).
pub async fn save(write_pool: &DbPool, blob: &str, license: &License) -> anyhow::Result<()> {
    let max_orgs = license
        .max_orgs
        .map(i32::try_from)
        .transpose()
        .map_err(|_| anyhow::anyhow!("license max_orgs exceeds i32::MAX"))?;
    let features = serde_json::to_string(
        &license
            .features
            .iter()
            .map(|f| f.wire_name())
            .collect::<Vec<_>>(),
    )?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(sql!(
        "INSERT INTO stackpit_license \
         (id, blob, license_id, customer, email, product, tier, issued_at, expires_at, features, max_orgs, activated_at, verified_at) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
         ON CONFLICT (id) DO UPDATE SET \
           blob = excluded.blob, license_id = excluded.license_id, customer = excluded.customer, \
           email = excluded.email, product = excluded.product, tier = excluded.tier, \
           issued_at = excluded.issued_at, expires_at = excluded.expires_at, features = excluded.features, \
           max_orgs = excluded.max_orgs, activated_at = excluded.activated_at, verified_at = excluded.verified_at"
    ))
    .bind(blob.trim())
    .bind(&license.license_id)
    .bind(&license.customer)
    .bind(&license.email)
    .bind(&license.product)
    .bind(&license.tier)
    .bind(license.issued_at.to_rfc3339())
    .bind(license.expires_at.map(|d| d.to_rfc3339()))
    .bind(features)
    .bind(max_orgs)
    .bind(&now)
    .bind(&now)
    .execute(write_pool)
    .await?;
    Ok(())
}

/// Delete the singleton row (operator-initiated deactivation).
pub async fn clear(write_pool: &DbPool) -> anyhow::Result<()> {
    sqlx::query(sql!("DELETE FROM stackpit_license WHERE id = 1"))
        .execute(write_pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn save_load_clear_roundtrip() {
        let pool = crate::db::open_test_pool().await;
        let lic = License {
            license_id: "abc".into(),
            customer: "Acme".into(),
            email: "a@acme.test".into(),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(365)),
            features: Vec::new(),
            max_orgs: Some(50),
            tier: "business".into(),
            product: "stackpit".into(),
        };
        save(&pool, "BLOBTEXT", &lic).await.unwrap();

        // load re-verifies the blob (won't verify here), so assert on the row.
        let row = sqlx::query(sql!(
            "SELECT customer, max_orgs, tier, product FROM stackpit_license WHERE id = 1"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("customer"), "Acme");
        assert_eq!(row.get::<i32, _>("max_orgs"), 50);
        assert_eq!(row.get::<String, _>("tier"), "business");
        assert_eq!(row.get::<String, _>("product"), "stackpit");

        // A different tier must persist as itself: these columns used to be
        // bound to the literals "stackpit" / "business" regardless of the blob.
        let pro = License {
            tier: "pro".into(),
            ..lic.clone()
        };
        save(&pool, "BLOBPRO", &pro).await.unwrap();
        let tier: String = sqlx::query(sql!("SELECT tier FROM stackpit_license WHERE id = 1"))
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("tier");
        assert_eq!(tier, "pro");

        // Re-save upserts the singleton rather than erroring on the PK.
        let lic2 = License {
            customer: "Beta".into(),
            ..lic.clone()
        };
        save(&pool, "BLOB2", &lic2).await.unwrap();
        let customer: String =
            sqlx::query(sql!("SELECT customer FROM stackpit_license WHERE id = 1"))
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("customer");
        assert_eq!(customer, "Beta");

        clear(&pool).await.unwrap();
        let none = sqlx::query(sql!("SELECT blob FROM stackpit_license WHERE id = 1"))
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(none.is_none());
    }
}
