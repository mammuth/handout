//! Owner authentication: API keys and OAuth access tokens (bearer), owner web sessions, password hashing, cookies.

use std::time::Duration;

use anyhow::Result;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::{
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::{
    app::AppState,
    config::Config,
    db::{self, Db},
    error::{ApiResult, AppError},
    ids,
    models::Owner,
};

pub fn hash_password(password: &str) -> Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Extractor: the owner behind `Authorization: Bearer …`.
pub struct OwnerAuth(pub Owner);

impl FromRequestParts<AppState> for OwnerAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(owner) = parts.extensions.get::<Owner>() {
            return Ok(OwnerAuth(owner.clone()));
        }
        let token = bearer_token(&parts.headers).ok_or_else(|| unauthorized(state))?;
        let owner = resolve_bearer(state, &token).await?;
        Ok(OwnerAuth(owner))
    }
}

pub fn unauthorized(state: &AppState) -> AppError {
    AppError::Unauthorized {
        resource_metadata: Some(format!(
            "{}/.well-known/oauth-protected-resource",
            state.cfg.public_url
        )),
    }
}

pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

pub async fn resolve_bearer(state: &AppState, token: &str) -> ApiResult<Owner> {
    let hash = ids::sha256_hex(token.as_bytes());
    let now = db::now();
    if token.starts_with("hk_") {
        let row: Option<(i64, Owner)> =
            sqlx::query_as::<_, (i64, i64, String, String, String, i64)>(
                "SELECT k.id, o.id, o.slug, o.name, o.password_hash, o.created_at \
             FROM api_keys k JOIN owners o ON o.id = k.owner_id \
             WHERE k.key_hash = ? AND k.revoked_at IS NULL",
            )
            .bind(&hash)
            .fetch_optional(&state.db)
            .await?
            .map(|(kid, id, slug, name, password_hash, created_at)| {
                (
                    kid,
                    Owner {
                        id,
                        slug,
                        name,
                        password_hash,
                        created_at,
                    },
                )
            });
        if let Some((key_id, owner)) = row {
            let db = state.db.clone();
            tokio::spawn(async move {
                let _ = sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(key_id)
                    .execute(&db)
                    .await;
            });
            return Ok(owner);
        }
        return Err(unauthorized(state));
    }
    let owner = sqlx::query_as::<_, Owner>(
        "SELECT o.* FROM oauth_tokens t JOIN owners o ON o.id = t.owner_id \
         WHERE t.token_hash = ? AND t.kind = 'access' AND t.revoked_at IS NULL AND t.expires_at > ?",
    )
    .bind(&hash)
    .bind(now)
    .fetch_optional(&state.db)
    .await?;
    owner.ok_or_else(|| unauthorized(state))
}

/// Middleware for routes that cannot use the extractor (the nested MCP service). Puts `Owner` into extensions.
pub async fn require_bearer(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(token) = bearer_token(req.headers()) else {
        return unauthorized(&state).into_response();
    };
    match resolve_bearer(&state, &token).await {
        Ok(owner) => {
            req.extensions_mut().insert(owner);
            next.run(req).await
        }
        Err(e) => e.into_response(),
    }
}

// ---- owner web sessions -------------------------------------------------------------------

pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

pub async fn create_session(db: &Db, owner_id: i64) -> Result<String> {
    let raw = ids::opaque_token();
    let now = db::now();
    sqlx::query("INSERT INTO sessions (id, owner_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(ids::sha256_hex(raw.as_bytes()))
        .bind(owner_id)
        .bind(now)
        .bind(now + SESSION_TTL.as_secs() as i64)
        .execute(db)
        .await?;
    Ok(raw)
}

pub async fn session_owner(db: &Db, raw: &str) -> Result<Option<Owner>> {
    let owner = sqlx::query_as::<_, Owner>(
        "SELECT o.* FROM sessions s JOIN owners o ON o.id = s.owner_id WHERE s.id = ? AND s.expires_at > ?",
    )
    .bind(ids::sha256_hex(raw.as_bytes()))
    .bind(db::now())
    .fetch_optional(db)
    .await?;
    Ok(owner)
}

pub async fn delete_session(db: &Db, raw: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(ids::sha256_hex(raw.as_bytes()))
        .execute(db)
        .await?;
    Ok(())
}

// ---- cookies ------------------------------------------------------------------------------

pub fn cookie(
    cfg: &Config,
    base: &str,
    value: String,
    max_age: Option<Duration>,
) -> Cookie<'static> {
    let mut b = Cookie::build((cfg.cookie_name(base), value))
        .path("/")
        .http_only(true)
        .secure(cfg.secure_cookies())
        .same_site(SameSite::Lax);
    if let Some(age) = max_age {
        b = b.max_age(time::Duration::seconds(age.as_secs() as i64));
    }
    b.build()
}

pub fn remove_cookie(cfg: &Config, base: &str) -> Cookie<'static> {
    Cookie::build((cfg.cookie_name(base), ""))
        .path("/")
        .http_only(true)
        .secure(cfg.secure_cookies())
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build()
}
