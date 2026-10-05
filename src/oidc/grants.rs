//! Server-side OIDC token vault. Cookie carries a 32-byte hex handle;
//! `oidc_grants.handle` PK is `SHA-256(raw_handle)` so a DB read yields
//! hashes, not replayable cookie values. Token columns are AES-GCM with
//! the raw handle as AAD -- blob-swapping between rows fails decryption.

use anyhow::{anyhow, Context, Result};
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use sqlx::Row;
use zeroize::Zeroize;

use crate::db::{sql, DbPool};
use crate::oidc::cookies::grant_cookie_name;
use crate::util::crypto::SecretEncryptor;

/// 32-byte opaque handle. Cookie carries the hex encoding (64 chars).
#[derive(Debug, Clone)]
pub struct GrantHandle(pub [u8; 32]);

impl GrantHandle {
    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        // OS RNG directly; this handle is the cookie's only secret.
        getrandom::fill(&mut bytes).expect("OS RNG must be available");
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let raw = hex::decode(s.trim()).ok()?;
        if raw.len() != 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&raw);
        Some(Self(out))
    }

    /// SHA-256 of the raw handle, used as the `oidc_grants.handle` PK.
    pub fn db_key(&self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }
}

/// New grant to persist after a successful auth-code exchange.
pub struct NewGrant<'a> {
    pub user_id: i64,
    pub iss: &'a str,
    pub sub: &'a str,
    pub sid: Option<&'a str>,
    pub access_token: &'a str,
    pub access_exp: i64,
    pub refresh_token: Option<&'a str>,
    pub refresh_exp: Option<i64>,
    pub id_token: &'a str,
    /// The OP granted `offline_access`; its refresh token outlives the session.
    pub offline: bool,
}

/// Row materialised from `oidc_grants` with tokens decrypted.
pub struct GrantRecord {
    pub handle: GrantHandle,
    pub user_id: i64,
    pub iss: String,
    pub sub: String,
    pub sid: Option<String>,
    pub access_token: String,
    pub access_exp: i64,
    pub refresh_token: Option<String>,
    pub refresh_exp: Option<i64>,
    pub id_token: Option<String>,
    /// Synchronizer CSRF token; compared against form field at every
    /// mutating /web/ request. Per-grant so it dies with the session.
    pub csrf_token: String,
    /// Unix timestamp when the grant row was first inserted.
    pub created_at: i64,
    pub offline: bool,
}

/// A refresh token that back-channel logout revokes (Back-Channel Logout 1.0
/// §2.7). `refresh_token_bc` decrypts with `hashed_handle` as AAD.
pub struct RevocableRefresh {
    /// The `oidc_grants.handle` column, [`GrantHandle::db_key`].
    pub hashed_handle: Vec<u8>,
    pub refresh_token_bc: Vec<u8>,
}

/// Copy of a session-bound refresh token readable without the cookie handle.
fn encrypt_backchannel_copy(
    encryptor: &SecretEncryptor,
    handle: &GrantHandle,
    refresh_token: &str,
) -> Result<Vec<u8>> {
    encryptor
        .encrypt_bytes_with_aad(refresh_token.as_bytes(), &handle.db_key())
        .ok_or_else(|| anyhow!("encrypting refresh_token back-channel copy failed"))
}

/// Generate a fresh 16-byte hex CSRF token from the OS RNG.
fn generate_csrf_token() -> String {
    crate::util::crypto::random_hex::<16>()
}

impl GrantRecord {
    /// True when the row should be refreshed before the next handler runs.
    pub fn should_refresh(&self, now_secs: i64, refresh_margin_secs: i64) -> bool {
        self.refresh_token.is_some() && self.access_exp - now_secs <= refresh_margin_secs
    }
}

/// Zeroize plaintext token bytes; the DB-side copy is encrypted.
impl Drop for GrantRecord {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(t) = self.refresh_token.as_mut() {
            t.zeroize();
        }
        if let Some(t) = self.id_token.as_mut() {
            t.zeroize();
        }
    }
}

/// Insert a new grant row, returning the freshly-generated handle.
pub async fn insert(
    pool: &DbPool,
    encryptor: &SecretEncryptor,
    new: &NewGrant<'_>,
) -> Result<GrantHandle> {
    let handle = GrantHandle::random();
    let now = chrono::Utc::now().timestamp();

    let access_ct = encryptor
        .encrypt_bytes_with_aad(new.access_token.as_bytes(), handle.as_bytes())
        .ok_or_else(|| anyhow!("encrypting access_token failed"))?;
    let refresh_ct = match new.refresh_token {
        Some(t) => Some(
            encryptor
                .encrypt_bytes_with_aad(t.as_bytes(), handle.as_bytes())
                .ok_or_else(|| anyhow!("encrypting refresh_token failed"))?,
        ),
        None => None,
    };
    let id_token_ct = encryptor
        .encrypt_bytes_with_aad(new.id_token.as_bytes(), handle.as_bytes())
        .ok_or_else(|| anyhow!("encrypting id_token failed"))?;
    let refresh_bc_ct = match new.refresh_token {
        Some(t) if !new.offline => Some(encrypt_backchannel_copy(encryptor, &handle, t)?),
        _ => None,
    };

    let db_key = handle.db_key();
    let csrf_token = generate_csrf_token();
    sqlx::query(sql!(
        "INSERT INTO oidc_grants \
         (handle, user_id, iss, sub, sid, access_token, access_exp, refresh_token, refresh_exp, id_token, csrf_token, key_id, created_at, last_used_at, offline, refresh_token_bc) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?12, ?13, ?14)"
    ))
    .bind(db_key.as_slice())
    .bind(new.user_id)
    .bind(new.iss)
    .bind(new.sub)
    .bind(new.sid)
    .bind(&access_ct)
    .bind(new.access_exp)
    .bind(refresh_ct.as_deref())
    .bind(new.refresh_exp)
    .bind(&id_token_ct)
    .bind(&csrf_token)
    .bind(now)
    .bind(new.offline)
    .bind(refresh_bc_ct.as_deref())
    .execute(pool)
    .await
    .context("inserting oidc_grants row")?;

    Ok(handle)
}

/// Load and decrypt a grant. `None` = no row (treat as logged-out).
/// Decryption failure is a hard error (key rotated or row tampered).
pub async fn load(
    pool: &DbPool,
    encryptor: &SecretEncryptor,
    handle: &GrantHandle,
) -> Result<Option<GrantRecord>> {
    let db_key = handle.db_key();
    let row = sqlx::query(sql!(
        "SELECT user_id, iss, sub, sid, access_token, access_exp, refresh_token, refresh_exp, id_token, csrf_token, created_at, offline \
         FROM oidc_grants WHERE handle = ?1"
    ))
    .bind(db_key.as_slice())
    .fetch_optional(pool)
    .await
    .context("loading oidc_grants row")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let access_ct: Vec<u8> = row.get("access_token");
    let access_pt = encryptor
        .decrypt_bytes_with_aad(&access_ct, handle.as_bytes())
        .ok_or_else(|| anyhow!("decrypting access_token failed (key rotation or tampering?)"))?;
    let access_token =
        String::from_utf8(access_pt).context("decrypted access_token is not UTF-8")?;

    let refresh_ct: Option<Vec<u8>> = row.get("refresh_token");
    let refresh_token = match refresh_ct {
        Some(ct) => {
            let pt = encryptor
                .decrypt_bytes_with_aad(&ct, handle.as_bytes())
                .ok_or_else(|| anyhow!("decrypting refresh_token failed"))?;
            Some(String::from_utf8(pt).context("decrypted refresh_token is not UTF-8")?)
        }
        None => None,
    };

    let id_token_ct: Option<Vec<u8>> = row.get("id_token");
    let id_token = match id_token_ct {
        Some(ct) => {
            let pt = encryptor
                .decrypt_bytes_with_aad(&ct, handle.as_bytes())
                .ok_or_else(|| anyhow!("decrypting id_token failed"))?;
            Some(String::from_utf8(pt).context("decrypted id_token is not UTF-8")?)
        }
        None => None,
    };

    // always set: populated at insert, backfilled for pre-008 rows at startup
    let csrf_token: String = row.get("csrf_token");

    Ok(Some(GrantRecord {
        handle: handle.clone(),
        user_id: row.get("user_id"),
        iss: row.get("iss"),
        sub: row.get("sub"),
        sid: row.get("sid"),
        access_token,
        access_exp: row.get("access_exp"),
        refresh_token,
        refresh_exp: row.get("refresh_exp"),
        id_token,
        csrf_token,
        created_at: row.get("created_at"),
        offline: row.get("offline"),
    }))
}

/// Resolve the grant cookie into a loaded [`GrantRecord`]: read cookie →
/// hex-decode the handle → [`load`]. `None` when the cookie is absent,
/// undecodable, or the row is missing; a load/decrypt error is logged and
/// also yields `None` (callers treat both as logged-out).
pub async fn resolve_from_headers(
    headers: &HeaderMap,
    secure: bool,
    encryptor: &SecretEncryptor,
    pool: &DbPool,
) -> Option<GrantRecord> {
    let handle = stackpit_auth::read_cookie(headers, grant_cookie_name(secure))
        .and_then(GrantHandle::from_hex)?;
    match load(pool, encryptor, &handle).await {
        Ok(record) => record,
        Err(e) => {
            tracing::error!("grant load failed: {e:#}");
            None
        }
    }
}

/// Clear a bad/expired grant in one line. Thin wrapper over [`delete`].
pub async fn forget(pool: &DbPool, handle: &GrantHandle) {
    let _ = delete(pool, handle).await;
}

/// One-shot startup fixup: mint a CSRF token for every pre-008 row that still
/// carries an empty `csrf_token`. New inserts always set one, so this only
/// touches rows that pre-date migration 008.
pub async fn backfill_csrf_tokens(pool: &DbPool) -> Result<u64> {
    let handles: Vec<Vec<u8>> =
        sqlx::query_scalar(sql!("SELECT handle FROM oidc_grants WHERE csrf_token = ''"))
            .fetch_all(pool)
            .await
            .context("listing oidc_grants needing csrf backfill")?;

    let mut updated = 0u64;
    for handle in handles {
        let token = generate_csrf_token();
        let res = sqlx::query(sql!(
            "UPDATE oidc_grants SET csrf_token = ?1 WHERE handle = ?2 AND csrf_token = ''"
        ))
        .bind(&token)
        .bind(handle.as_slice())
        .execute(pool)
        .await
        .context("backfilling csrf_token on oidc_grants row")?;
        updated += res.rows_affected();
    }
    Ok(updated)
}

/// Rotate tokens on a successful refresh. `id_token` is the refreshed ID
/// token to keep, if any; `None` keeps the stored one. `offline` is `Some`
/// when the refresh response listed `scope`; `None` keeps the stored flag.
pub async fn rotate_tokens(
    pool: &DbPool,
    encryptor: &SecretEncryptor,
    handle: &GrantHandle,
    tokens: &crate::oidc::client::RefreshSuccess,
    id_token: Option<&str>,
    offline: Option<bool>,
) -> Result<()> {
    let access_ct = encryptor
        .encrypt_bytes_with_aad(tokens.access_token.as_bytes(), handle.as_bytes())
        .ok_or_else(|| anyhow!("encrypting refreshed access_token failed"))?;
    let refresh_ct = match tokens.refresh_token.as_deref() {
        Some(t) => Some(
            encryptor
                .encrypt_bytes_with_aad(t.as_bytes(), handle.as_bytes())
                .ok_or_else(|| anyhow!("encrypting refreshed refresh_token failed"))?,
        ),
        None => None,
    };
    let refresh_bc_ct = match tokens.refresh_token.as_deref() {
        Some(t) => Some(encrypt_backchannel_copy(encryptor, handle, t)?),
        None => None,
    };
    let id_token_ct = match id_token {
        Some(t) => Some(
            encryptor
                .encrypt_bytes_with_aad(t.as_bytes(), handle.as_bytes())
                .ok_or_else(|| anyhow!("encrypting refreshed id_token failed"))?,
        ),
        None => None,
    };
    let now = chrono::Utc::now().timestamp();
    let db_key = handle.db_key();

    // SET expressions read the pre-update row, so `offline` here is the stored flag.
    sqlx::query(sql!(
        "UPDATE oidc_grants SET access_token = ?1, access_exp = ?2, \
         refresh_token = COALESCE(?3, refresh_token), refresh_exp = COALESCE(?4, refresh_exp), \
         id_token = COALESCE(?7, id_token), last_used_at = ?5, \
         offline = COALESCE(?8, offline), \
         refresh_token_bc = CASE WHEN COALESCE(?8, offline) THEN NULL \
                                 ELSE COALESCE(?9, refresh_token_bc) END \
         WHERE handle = ?6"
    ))
    .bind(&access_ct)
    .bind(tokens.access_exp)
    .bind(refresh_ct.as_deref())
    .bind(tokens.refresh_exp)
    .bind(now)
    .bind(db_key.as_slice())
    .bind(id_token_ct.as_deref())
    .bind(offline)
    .bind(refresh_bc_ct.as_deref())
    .execute(pool)
    .await
    .context("updating oidc_grants row")?;
    Ok(())
}

/// Delete a single grant by handle. Returns the number of rows affected.
pub async fn delete(pool: &DbPool, handle: &GrantHandle) -> Result<u64> {
    let db_key = handle.db_key();
    let res = sqlx::query(sql!("DELETE FROM oidc_grants WHERE handle = ?1"))
        .bind(db_key.as_slice())
        .execute(pool)
        .await
        .context("deleting oidc_grants row")?;
    Ok(res.rows_affected())
}

/// Revocable refresh tokens of the grants matching `(iss, sid)`.
pub async fn select_revocable_by_sid(
    pool: &DbPool,
    iss: &str,
    sid: &str,
) -> Result<Vec<RevocableRefresh>> {
    let rows = sqlx::query(sql!(
        "SELECT handle, refresh_token_bc FROM oidc_grants \
         WHERE iss = ?1 AND sid = ?2 AND NOT offline AND refresh_token_bc IS NOT NULL"
    ))
    .bind(iss)
    .bind(sid)
    .fetch_all(pool)
    .await
    .context("selecting revocable oidc_grants by sid")?;
    Ok(rows.iter().map(revocable_from_row).collect())
}

/// Revocable refresh tokens of the grants matching `(iss, sub)`.
pub async fn select_revocable_by_sub(
    pool: &DbPool,
    iss: &str,
    sub: &str,
) -> Result<Vec<RevocableRefresh>> {
    let rows = sqlx::query(sql!(
        "SELECT handle, refresh_token_bc FROM oidc_grants \
         WHERE iss = ?1 AND sub = ?2 AND NOT offline AND refresh_token_bc IS NOT NULL"
    ))
    .bind(iss)
    .bind(sub)
    .fetch_all(pool)
    .await
    .context("selecting revocable oidc_grants by sub")?;
    Ok(rows.iter().map(revocable_from_row).collect())
}

fn revocable_from_row(row: &crate::db::DbRow) -> RevocableRefresh {
    RevocableRefresh {
        hashed_handle: row.get("handle"),
        refresh_token_bc: row.get("refresh_token_bc"),
    }
}

/// Delete every grant matching `(iss, sid)` -- sid-scoped (single device)
/// back-channel logout. NOP if `sid` is empty.
pub async fn delete_by_sid(pool: &DbPool, iss: &str, sid: &str) -> Result<u64> {
    if sid.is_empty() {
        return Ok(0);
    }
    let res = sqlx::query(sql!("DELETE FROM oidc_grants WHERE iss = ?1 AND sid = ?2"))
        .bind(iss)
        .bind(sid)
        .execute(pool)
        .await
        .context("deleting oidc_grants by sid")?;
    Ok(res.rows_affected())
}

/// Delete every grant matching `(iss, sub)`. Whole-user logout.
pub async fn delete_by_sub(pool: &DbPool, iss: &str, sub: &str) -> Result<u64> {
    let res = sqlx::query(sql!("DELETE FROM oidc_grants WHERE iss = ?1 AND sub = ?2"))
        .bind(iss)
        .bind(sub)
        .execute(pool)
        .await
        .context("deleting oidc_grants by sub")?;
    Ok(res.rows_affected())
}

/// Purge grants whose refresh_token (or access_token, if no refresh) has expired.
/// Hydra omits `refresh_token_exp`, so a refresh grant with no `refresh_exp`
/// goes once it has sat unused past `refresh_max_ttl_secs`.
pub async fn purge_expired(pool: &DbPool, now_secs: i64, refresh_max_ttl_secs: i64) -> Result<u64> {
    let res = sqlx::query(sql!(
        "DELETE FROM oidc_grants WHERE \
         (refresh_exp IS NOT NULL AND refresh_exp <= ?1) OR \
         (refresh_token IS NULL AND access_exp <= ?1) OR \
         (refresh_token IS NOT NULL AND refresh_exp IS NULL AND last_used_at <= ?2)"
    ))
    .bind(now_secs)
    .bind(now_secs.saturating_sub(refresh_max_ttl_secs))
    .execute(pool)
    .await
    .context("purging expired oidc_grants")?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod purge_tests {
    use super::*;

    async fn seed_grant(pool: &DbPool, user_id: i64, handle: &[u8], last_used_at: i64) {
        sqlx::query(sql!(
            "INSERT INTO oidc_grants \
             (handle, user_id, iss, sub, access_token, access_exp, refresh_token, refresh_exp, created_at, last_used_at) \
             VALUES (?1, ?2, 'https://idp', 'sub', ?3, 0, ?3, NULL, ?4, ?4)"
        ))
        .bind(handle)
        .bind(user_id)
        .bind(b"ct".as_slice())
        .bind(last_used_at)
        .execute(pool)
        .await
        .unwrap();
    }

    /// C20 (round-3 review): Hydra sends no `refresh_token_exp`, so a refresh
    /// grant had `refresh_exp = NULL` and matched neither purge arm, forever.
    #[tokio::test]
    async fn refresh_grants_without_refresh_exp_purge_once_idle_past_the_ttl() {
        let pool = crate::db::open_test_pool().await;
        let user_id =
            crate::queries::users::upsert_from_oidc(&pool, "https://idp", "sub", None, None)
                .await
                .unwrap()
                .user_id;
        let now = 1_900_000_000;
        let ttl = 14 * 24 * 3600;
        seed_grant(&pool, user_id, b"stale", now - ttl - 1).await;
        seed_grant(&pool, user_id, b"fresh", now - 60).await;

        assert_eq!(purge_expired(&pool, now, ttl).await.unwrap(), 1);
        let left: i64 = sqlx::query_scalar(sql!("SELECT COUNT(*) FROM oidc_grants"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 1);
    }

    #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
    #[tokio::test]
    async fn refresh_updates_stored_id_token() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let u = crate::queries::users::upsert_from_oidc(&pool, "https://idp", "sub-rt", None, None)
            .await
            .unwrap();
        let enc = SecretEncryptor::from_config_or_env(Some(&secrecy::SecretString::from(
            "11".repeat(32),
        )))
        .unwrap()
        .unwrap();
        let new = |id_token| NewGrant {
            user_id: u.user_id,
            iss: "https://idp",
            sub: "sub-rt",
            sid: None,
            access_token: "at",
            access_exp: 0,
            refresh_token: Some("rt"),
            refresh_exp: None,
            id_token,
            offline: false,
        };
        let handle = insert(&pool, &enc, &new("old-id-token")).await.unwrap();

        let refreshed = |at: &str, exp| crate::oidc::client::RefreshSuccess {
            access_token: at.to_string(),
            access_exp: exp,
            refresh_token: None,
            refresh_exp: None,
            id_token: None,
            offline: None,
        };
        rotate_tokens(&pool, &enc, &handle, &refreshed("at2", 1), None, None)
            .await
            .unwrap();
        let g = load(&pool, &enc, &handle).await.unwrap().unwrap();
        assert_eq!(g.id_token.as_deref(), Some("old-id-token"));

        rotate_tokens(
            &pool,
            &enc,
            &handle,
            &refreshed("at3", 2),
            Some("new-id-token"),
            None,
        )
        .await
        .unwrap();
        let g = load(&pool, &enc, &handle).await.unwrap().unwrap();
        assert_eq!(g.id_token.as_deref(), Some("new-id-token"));
        assert_eq!(g.refresh_token.as_deref(), Some("rt"));
    }
}

#[cfg(all(test, feature = "sqlite", not(feature = "postgres")))]
mod backchannel_copy_tests {
    use super::*;

    const ISS: &str = "https://idp";

    fn encryptor() -> SecretEncryptor {
        SecretEncryptor::from_config_or_env(Some(&secrecy::SecretString::from("11".repeat(32))))
            .unwrap()
            .unwrap()
    }

    async fn insert_grant(
        pool: &DbPool,
        enc: &SecretEncryptor,
        sid: &str,
        refresh_token: Option<&str>,
        offline: bool,
    ) -> GrantHandle {
        let user_id = crate::queries::users::upsert_from_oidc(pool, ISS, "alice", None, None)
            .await
            .unwrap()
            .user_id;
        insert(
            pool,
            enc,
            &NewGrant {
                user_id,
                iss: ISS,
                sub: "alice",
                sid: Some(sid),
                access_token: "at",
                access_exp: 0,
                refresh_token,
                refresh_exp: None,
                id_token: "it",
                offline,
            },
        )
        .await
        .unwrap()
    }

    async fn stored_copy(pool: &DbPool, handle: &GrantHandle) -> Option<Vec<u8>> {
        sqlx::query_scalar(sql!(
            "SELECT refresh_token_bc FROM oidc_grants WHERE handle = ?1"
        ))
        .bind(handle.db_key().as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn decrypt_copy(enc: &SecretEncryptor, handle: &GrantHandle, ct: &[u8]) -> String {
        String::from_utf8(enc.decrypt_bytes_with_aad(ct, &handle.db_key()).unwrap()).unwrap()
    }

    async fn rotate(
        pool: &DbPool,
        enc: &SecretEncryptor,
        handle: &GrantHandle,
        refresh_token: Option<&str>,
        offline: Option<bool>,
    ) {
        let tokens = crate::oidc::client::RefreshSuccess {
            access_token: "at2".to_string(),
            access_exp: 1,
            refresh_token: refresh_token.map(str::to_string),
            refresh_exp: None,
            id_token: None,
            offline,
        };
        rotate_tokens(pool, enc, handle, &tokens, None, offline)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn insert_stores_the_backchannel_copy_only_for_session_bound_grants() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = encryptor();
        let session = insert_grant(&pool, &enc, "s1", Some("rt-session"), false).await;
        let offline = insert_grant(&pool, &enc, "s2", Some("rt-offline"), true).await;

        let ct = stored_copy(&pool, &session).await.unwrap();
        assert_eq!(decrypt_copy(&enc, &session, &ct), "rt-session");
        assert!(enc
            .decrypt_bytes_with_aad(&ct, session.as_bytes())
            .is_none());
        assert!(stored_copy(&pool, &offline).await.is_none());

        assert!(!load(&pool, &enc, &session).await.unwrap().unwrap().offline);
        assert!(load(&pool, &enc, &offline).await.unwrap().unwrap().offline);
    }

    #[tokio::test]
    async fn rotate_with_offline_false_and_a_new_refresh_token_rewrites_the_copy() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = encryptor();
        let handle = insert_grant(&pool, &enc, "s1", Some("rt1"), false).await;

        rotate(&pool, &enc, &handle, Some("rt2"), Some(false)).await;

        let ct = stored_copy(&pool, &handle).await.unwrap();
        assert_eq!(decrypt_copy(&enc, &handle, &ct), "rt2");
        assert!(!load(&pool, &enc, &handle).await.unwrap().unwrap().offline);
    }

    #[tokio::test]
    async fn rotate_with_offline_true_clears_the_copy() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = encryptor();
        let handle = insert_grant(&pool, &enc, "s1", Some("rt1"), false).await;

        rotate(&pool, &enc, &handle, None, Some(true)).await;

        assert!(stored_copy(&pool, &handle).await.is_none());
        assert!(load(&pool, &enc, &handle).await.unwrap().unwrap().offline);

        rotate(&pool, &enc, &handle, Some("rt2"), None).await;
        assert!(stored_copy(&pool, &handle).await.is_none());
    }

    #[tokio::test]
    async fn rotate_without_scope_or_refresh_token_keeps_the_copy() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = encryptor();
        let handle = insert_grant(&pool, &enc, "s1", Some("rt1"), false).await;
        let before = stored_copy(&pool, &handle).await.unwrap();

        rotate(&pool, &enc, &handle, None, None).await;

        assert_eq!(stored_copy(&pool, &handle).await.unwrap(), before);
        assert!(!load(&pool, &enc, &handle).await.unwrap().unwrap().offline);
    }

    #[tokio::test]
    async fn select_revocable_returns_only_session_bound_grants_with_a_refresh_token() {
        let pool = crate::queries::test_helpers::open_test_db().await;
        let enc = encryptor();
        insert_grant(&pool, &enc, "s1", Some("rt-offline"), true).await;
        insert_grant(&pool, &enc, "s1", None, false).await;
        let revocable = insert_grant(&pool, &enc, "s1", Some("rt-session"), false).await;

        for rows in [
            select_revocable_by_sid(&pool, ISS, "s1").await.unwrap(),
            select_revocable_by_sub(&pool, ISS, "alice").await.unwrap(),
        ] {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].hashed_handle, revocable.db_key());
            let pt = enc
                .decrypt_bytes_with_aad(&rows[0].refresh_token_bc, &rows[0].hashed_handle)
                .unwrap();
            assert_eq!(pt, b"rt-session");
        }
        assert!(select_revocable_by_sid(&pool, ISS, "other")
            .await
            .unwrap()
            .is_empty());
    }
}
