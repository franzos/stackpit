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

    let db_key = handle.db_key();
    let csrf_token = generate_csrf_token();
    sqlx::query(sql!(
        "INSERT INTO oidc_grants \
         (handle, user_id, iss, sub, sid, access_token, access_exp, refresh_token, refresh_exp, id_token, csrf_token, key_id, created_at, last_used_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?12)"
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
        "SELECT user_id, iss, sub, sid, access_token, access_exp, refresh_token, refresh_exp, id_token, csrf_token, created_at \
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

/// Rotate tokens on a successful refresh.
pub async fn rotate_tokens(
    pool: &DbPool,
    encryptor: &SecretEncryptor,
    handle: &GrantHandle,
    access_token: &str,
    access_exp: i64,
    refresh_token: Option<&str>,
    refresh_exp: Option<i64>,
) -> Result<()> {
    let access_ct = encryptor
        .encrypt_bytes_with_aad(access_token.as_bytes(), handle.as_bytes())
        .ok_or_else(|| anyhow!("encrypting refreshed access_token failed"))?;
    let refresh_ct = match refresh_token {
        Some(t) => Some(
            encryptor
                .encrypt_bytes_with_aad(t.as_bytes(), handle.as_bytes())
                .ok_or_else(|| anyhow!("encrypting refreshed refresh_token failed"))?,
        ),
        None => None,
    };
    let now = chrono::Utc::now().timestamp();
    let db_key = handle.db_key();

    sqlx::query(sql!(
        "UPDATE oidc_grants SET access_token = ?1, access_exp = ?2, \
         refresh_token = COALESCE(?3, refresh_token), refresh_exp = COALESCE(?4, refresh_exp), \
         last_used_at = ?5 WHERE handle = ?6"
    ))
    .bind(&access_ct)
    .bind(access_exp)
    .bind(refresh_ct.as_deref())
    .bind(refresh_exp)
    .bind(now)
    .bind(db_key.as_slice())
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
pub async fn purge_expired(pool: &DbPool, now_secs: i64) -> Result<u64> {
    let res = sqlx::query(sql!(
        "DELETE FROM oidc_grants WHERE \
         (refresh_exp IS NOT NULL AND refresh_exp <= ?1) OR \
         (refresh_token IS NULL AND access_exp <= ?1)"
    ))
    .bind(now_secs)
    .execute(pool)
    .await
    .context("purging expired oidc_grants")?;
    Ok(res.rows_affected())
}
