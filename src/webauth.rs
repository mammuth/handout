//! Owner web UI auth: session login (password or API key) and short-lived bearer tokens for the SPA.
//! `/api/*` stays bearer-only; the SPA mints a token from its session cookie and uses it like an agent.

use std::{net::SocketAddr, time::Duration};

use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    app::AppState,
    auth,
    db::{self, Db},
    error::{ApiResult, AppError},
    ids,
    models::Owner,
    oauth::SESSION_COOKIE,
    service, shell,
    view::{check_origin, client_ip},
};

pub const WEB_CLIENT: &str = "web";
const WEB_TOKEN_TTL: i64 = 3600;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/token", post(token))
        .route("/auth/logout", post(logout))
        .route("/admin", get(|| async { shell::spa_index() }))
        .route("/admin/{*rest}", get(|| async { shell::spa_index() }))
}

#[derive(Deserialize)]
struct LoginBody {
    slug: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
}

/// Resolve an owner from slug+password, or from an API key (given explicitly or pasted into the password field).
pub async fn authenticate(st: &AppState, slug: Option<&str>, secret: &str) -> ApiResult<Owner> {
    let secret = secret.trim();
    if secret.starts_with("hk_") {
        return auth::resolve_api_key(st, secret)
            .await
            .map_err(|_| AppError::Forbidden);
    }
    let slug = slug
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Forbidden)?;
    let owner = service::owner_by_slug(&st.db, slug)
        .await?
        .ok_or(AppError::Forbidden)?;
    if auth::verify_password(secret, &owner.password_hash) {
        Ok(owner)
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn issue_web_token(db: &Db, owner_id: i64) -> ApiResult<(String, i64)> {
    let token = ids::opaque_token();
    sqlx::query("INSERT INTO oauth_tokens (token_hash, kind, client_id, owner_id, scope, expires_at) VALUES (?, 'access', ?, ?, 'handout', ?)")
        .bind(ids::sha256_hex(token.as_bytes()))
        .bind(WEB_CLIENT)
        .bind(owner_id)
        .bind(db::now() + WEB_TOKEN_TTL)
        .execute(db)
        .await?;
    Ok((token, WEB_TOKEN_TTL))
}

fn session_payload(st: &AppState, owner: &Owner, token: String, ttl: i64) -> Value {
    json!({ "owner": service::owner_json(st, owner), "access_token": token, "expires_in": ttl })
}

async fn login(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> ApiResult<Response> {
    check_origin(&st, &headers)?;
    let ip = client_ip(&headers, Some(addr));
    if !st
        .limiter
        .check(&format!("login:{ip}"), 10, Duration::from_secs(900))
    {
        return Err(AppError::RateLimited);
    }
    let secret = body.api_key.or(body.password).unwrap_or_default();
    let owner = authenticate(&st, body.slug.as_deref(), &secret).await?;
    let raw = auth::create_session(&st.db, owner.id).await?;
    let (token, ttl) = issue_web_token(&st.db, owner.id).await?;
    let jar = jar.add(auth::cookie(
        &st.cfg,
        SESSION_COOKIE,
        raw,
        Some(auth::SESSION_TTL),
    ));
    Ok((jar, Json(session_payload(&st, &owner, token, ttl))).into_response())
}

async fn session_owner(st: &AppState, jar: &CookieJar) -> ApiResult<(String, Owner)> {
    let raw = jar
        .get(&st.cfg.cookie_name(SESSION_COOKIE))
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized {
            resource_metadata: None,
        })?;
    let owner = auth::session_owner(&st.db, &raw)
        .await?
        .ok_or(AppError::Unauthorized {
            resource_metadata: None,
        })?;
    Ok((raw, owner))
}

async fn token(
    State(st): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<Json<Value>> {
    check_origin(&st, &headers)?;
    let (_, owner) = session_owner(&st, &jar).await?;
    let (token, ttl) = issue_web_token(&st.db, owner.id).await?;
    Ok(Json(session_payload(&st, &owner, token, ttl)))
}

async fn logout(
    State(st): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> ApiResult<Response> {
    check_origin(&st, &headers)?;
    if let Ok((raw, owner)) = session_owner(&st, &jar).await {
        auth::delete_session(&st.db, &raw).await?;
        sqlx::query("UPDATE oauth_tokens SET revoked_at = ? WHERE owner_id = ? AND client_id = ? AND revoked_at IS NULL")
            .bind(db::now())
            .bind(owner.id)
            .bind(WEB_CLIENT)
            .execute(&st.db)
            .await?;
    }
    let jar = jar.add(auth::remove_cookie(&st.cfg, SESSION_COOKIE));
    Ok((jar, Json(json!({ "ok": true }))).into_response())
}
