//! Minimal OAuth 2.1 authorization server for MCP clients (claude.ai, Claude Code):
//! metadata (RFC 8414 / 9728), dynamic client registration (RFC 7591), authorization code + PKCE S256,
//! refresh tokens, owner login + consent pages.

use std::time::Duration;

use axum::{
    Form, Json, Router,
    extract::{ConnectInfo, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;

use crate::{
    app::AppState,
    auth,
    db::{self},
    error::{ApiResult, AppError},
    ids,
    markdown::escape,
    models::Owner,
    view,
};

pub const SESSION_COOKIE: &str = "hs";
const CODE_TTL: i64 = 300;
const ACCESS_TTL: i64 = 3600;
const REFRESH_TTL: i64 = 30 * 24 * 3600;
const SCOPE: &str = "handout";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(as_metadata))
        .route(
            "/.well-known/oauth-authorization-server/{*rest}",
            get(as_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/{*rest}",
            get(resource_metadata),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize_get).post(authorize_post))
        .route("/oauth/token", post(token))
        .route("/login", get(login_get).post(login_post))
        .route("/logout", post(logout))
}

fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn as_metadata(State(st): State<AppState>) -> Json<Value> {
    let b = &st.cfg.public_url;
    Json(json!({
        "issuer": b,
        "authorization_endpoint": format!("{b}/oauth/authorize"),
        "token_endpoint": format!("{b}/oauth/token"),
        "registration_endpoint": format!("{b}/oauth/register"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": [SCOPE],
    }))
}

async fn resource_metadata(State(st): State<AppState>) -> Json<Value> {
    let b = &st.cfg.public_url;
    Json(json!({
        "resource": format!("{b}/mcp"),
        "authorization_servers": [b],
        "bearer_methods_supported": ["header"],
        "scopes_supported": [SCOPE],
        "resource_name": "handout",
    }))
}

#[derive(Deserialize)]
struct RegisterBody {
    client_name: Option<String>,
    redirect_uris: Vec<String>,
}

fn redirect_uri_ok(u: &str) -> bool {
    u.starts_with("https://")
        || u.starts_with("http://localhost")
        || u.starts_with("http://127.0.0.1")
        || u.starts_with("http://[::1]")
}

async fn register(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: header::HeaderMap,
    Json(body): Json<RegisterBody>,
) -> ApiResult<Response> {
    let ip = view::client_ip(&headers, Some(addr));
    if !st
        .limiter
        .check(&format!("register:{ip}"), 20, Duration::from_secs(3600))
    {
        return Err(AppError::RateLimited);
    }
    if body.redirect_uris.is_empty() || !body.redirect_uris.iter().all(|u| redirect_uri_ok(u)) {
        return Ok(oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uris must be https or loopback http",
        ));
    }
    let client_id = ids::opaque_token();
    let name = body
        .client_name
        .unwrap_or_else(|| "MCP client".into())
        .chars()
        .take(80)
        .collect::<String>();
    let now = db::now();
    sqlx::query("INSERT INTO oauth_clients (client_id, client_name, redirect_uris, created_at) VALUES (?, ?, ?, ?)")
        .bind(&client_id)
        .bind(&name)
        .bind(serde_json::to_string(&body.redirect_uris).unwrap())
        .bind(now)
        .execute(&st.db)
        .await?;
    Ok(no_store(
        (
            StatusCode::CREATED,
            Json(json!({
                "client_id": client_id,
                "client_name": name,
                "redirect_uris": body.redirect_uris,
                "token_endpoint_auth_method": "none",
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "client_id_issued_at": now,
            })),
        )
            .into_response(),
    ))
}

fn oauth_error(status: StatusCode, code: &str, desc: &str) -> Response {
    no_store(
        (
            status,
            Json(json!({ "error": code, "error_description": desc })),
        )
            .into_response(),
    )
}

#[derive(sqlx::FromRow)]
struct Client {
    client_name: String,
    redirect_uris: String,
}

async fn load_client(st: &AppState, client_id: &str) -> ApiResult<Option<Client>> {
    Ok(sqlx::query_as::<_, Client>(
        "SELECT client_name, redirect_uris FROM oauth_clients WHERE client_id = ?",
    )
    .bind(client_id)
    .fetch_optional(&st.db)
    .await?)
}

impl Client {
    fn allows(&self, redirect_uri: &str) -> bool {
        serde_json::from_str::<Vec<String>>(&self.redirect_uris)
            .map(|v| v.iter().any(|u| u == redirect_uri))
            .unwrap_or(false)
    }
}

#[derive(Deserialize, Clone)]
struct AuthorizeParams {
    response_type: Option<String>,
    client_id: String,
    redirect_uri: String,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    resource: Option<String>,
}

impl AuthorizeParams {
    fn query_string(&self) -> String {
        let mut parts = vec![
            format!("response_type={}", enc("code")),
            format!("client_id={}", enc(&self.client_id)),
            format!("redirect_uri={}", enc(&self.redirect_uri)),
        ];
        if let Some(v) = &self.code_challenge {
            parts.push(format!("code_challenge={}", enc(v)));
        }
        if let Some(v) = &self.code_challenge_method {
            parts.push(format!("code_challenge_method={}", enc(v)));
        }
        if let Some(v) = &self.state {
            parts.push(format!("state={}", enc(v)));
        }
        if let Some(v) = &self.scope {
            parts.push(format!("scope={}", enc(v)));
        }
        if let Some(v) = &self.resource {
            parts.push(format!("resource={}", enc(v)));
        }
        parts.join("&")
    }
}

/// Percent-encode for query strings (RFC 3986 unreserved kept).
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn redirect_with(redirect_uri: &str, params: &[(&str, &str)]) -> Response {
    let sep = if redirect_uri.contains('?') { '&' } else { '?' };
    let qs = params
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&");
    Redirect::to(&format!("{redirect_uri}{sep}{qs}")).into_response()
}

async fn session_owner(st: &AppState, jar: &CookieJar) -> Option<Owner> {
    let raw = jar
        .get(&st.cfg.cookie_name(SESSION_COOKIE))?
        .value()
        .to_string();
    auth::session_owner(&st.db, &raw).await.ok().flatten()
}

#[allow(clippy::result_large_err)]
async fn validate_authorize(st: &AppState, p: &AuthorizeParams) -> Result<Client, Response> {
    let client = match load_client(st, &p.client_id).await {
        Ok(Some(c)) => c,
        _ => {
            return Err(page(
                "Unknown client",
                "<p>This OAuth client is not registered.</p>",
            ));
        }
    };
    if !client.allows(&p.redirect_uri) {
        return Err(page(
            "Invalid redirect",
            "<p>The redirect URI is not registered for this client.</p>",
        ));
    }
    if p.response_type.as_deref() != Some("code") {
        return Err(redirect_with(
            &p.redirect_uri,
            &[
                ("error", "unsupported_response_type"),
                ("state", p.state.as_deref().unwrap_or("")),
            ],
        ));
    }
    if p.code_challenge
        .as_deref()
        .map(|c| c.len() < 43)
        .unwrap_or(true)
        || p.code_challenge_method.as_deref() != Some("S256")
    {
        return Err(redirect_with(
            &p.redirect_uri,
            &[
                ("error", "invalid_request"),
                ("error_description", "PKCE S256 code_challenge required"),
                ("state", p.state.as_deref().unwrap_or("")),
            ],
        ));
    }
    Ok(client)
}

async fn authorize_get(
    State(st): State<AppState>,
    Query(p): Query<AuthorizeParams>,
    jar: CookieJar,
) -> Response {
    let client = match validate_authorize(&st, &p).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(owner) = session_owner(&st, &jar).await else {
        let next = format!("/oauth/authorize?{}", p.query_string());
        return Redirect::to(&format!("/login?next={}", enc(&next))).into_response();
    };
    let body = format!(
        r#"<p class="muted">Signed in as <strong>{owner}</strong> (<code>{slug}</code>)</p>
<h2>Allow <em>{client}</em> to act as you?</h2>
<p>It will be able to upload files, manage recipients and read opens through the MCP server.</p>
<form method="post" action="/oauth/authorize?{qs}">
  <button name="decision" value="allow" class="primary">Allow</button>
  <button name="decision" value="deny">Deny</button>
</form>
<form method="post" action="/logout" class="tiny"><button>Not you? Sign out</button></form>"#,
        owner = escape(&owner.name),
        slug = escape(&owner.slug),
        client = escape(&client.client_name),
        qs = p.query_string(),
    );
    let mut res = page("Authorize", &body);
    if let Some(src) = csp_source(&p.redirect_uri) {
        let csp = crate::app::shell_csp(&st.cfg.content_url, &format!(" {src}"));
        if let Ok(v) = HeaderValue::from_str(&csp) {
            res.headers_mut().insert(header::CONTENT_SECURITY_POLICY, v);
        }
    }
    res
}

/// CSP source for a redirect URI: `scheme://host[:port]` for http(s), `scheme:` for native-app schemes.
/// Anything that could smuggle extra CSP tokens yields `None`.
fn csp_source(uri: &str) -> Option<String> {
    let (scheme, rest) = uri.split_once(':')?;
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
    if !scheme_ok {
        return None;
    }
    if scheme != "http" && scheme != "https" {
        return Some(format!("{scheme}:"));
    }
    let authority = rest.strip_prefix("//")?.split(['/', '?', '#']).next()?;
    let authority_ok = !authority.is_empty()
        && authority.chars().all(|c| c.is_ascii_alphanumeric() || "-.:[]".contains(c));
    authority_ok.then(|| format!("{scheme}://{authority}"))
}

#[derive(Deserialize)]
struct Decision {
    decision: String,
}

async fn authorize_post(
    State(st): State<AppState>,
    Query(p): Query<AuthorizeParams>,
    jar: CookieJar,
    Form(d): Form<Decision>,
) -> Response {
    if let Err(r) = validate_authorize(&st, &p).await {
        return r;
    }
    let Some(owner) = session_owner(&st, &jar).await else {
        return Redirect::to(&format!(
            "/login?next={}",
            enc(&format!("/oauth/authorize?{}", p.query_string()))
        ))
        .into_response();
    };
    let state = p.state.clone().unwrap_or_default();
    if d.decision != "allow" {
        return redirect_with(
            &p.redirect_uri,
            &[("error", "access_denied"), ("state", &state)],
        );
    }
    let code = ids::opaque_token();
    let res = sqlx::query(
        "INSERT INTO oauth_codes (code_hash, client_id, owner_id, redirect_uri, code_challenge, scope, resource, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(ids::sha256_hex(code.as_bytes()))
    .bind(&p.client_id)
    .bind(owner.id)
    .bind(&p.redirect_uri)
    .bind(p.code_challenge.as_deref().unwrap_or(""))
    .bind(SCOPE)
    .bind(&p.resource)
    .bind(db::now() + CODE_TTL)
    .execute(&st.db)
    .await;
    if res.is_err() {
        return redirect_with(
            &p.redirect_uri,
            &[("error", "server_error"), ("state", &state)],
        );
    }
    redirect_with(&p.redirect_uri, &[("code", &code), ("state", &state)])
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

async fn issue_tokens(st: &AppState, client_id: &str, owner_id: i64) -> ApiResult<Value> {
    let access = ids::opaque_token();
    let refresh = ids::opaque_token();
    let now = db::now();
    let mut tx = st.db.begin().await?;
    sqlx::query("INSERT INTO oauth_tokens (token_hash, kind, client_id, owner_id, scope, expires_at) VALUES (?, 'access', ?, ?, ?, ?)")
        .bind(ids::sha256_hex(access.as_bytes()))
        .bind(client_id)
        .bind(owner_id)
        .bind(SCOPE)
        .bind(now + ACCESS_TTL)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO oauth_tokens (token_hash, kind, client_id, owner_id, scope, expires_at, parent_hash) VALUES (?, 'refresh', ?, ?, ?, ?, ?)")
        .bind(ids::sha256_hex(refresh.as_bytes()))
        .bind(client_id)
        .bind(owner_id)
        .bind(SCOPE)
        .bind(now + REFRESH_TTL)
        .bind(ids::sha256_hex(access.as_bytes()))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(
        json!({ "access_token": access, "token_type": "Bearer", "expires_in": ACCESS_TTL, "refresh_token": refresh, "scope": SCOPE }),
    )
}

async fn token(State(st): State<AppState>, Form(f): Form<TokenForm>) -> ApiResult<Response> {
    match f.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier), Some(client_id)) =
                (f.code, f.code_verifier, f.client_id)
            else {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "code, code_verifier and client_id are required",
                ));
            };
            let row = sqlx::query_as::<_, (String, i64, String, String, i64, Option<i64>)>(
                "SELECT client_id, owner_id, redirect_uri, code_challenge, expires_at, used_at FROM oauth_codes WHERE code_hash = ?",
            )
            .bind(ids::sha256_hex(code.as_bytes()))
            .fetch_optional(&st.db)
            .await?;
            let Some((cid, owner_id, redirect_uri, challenge, expires_at, used_at)) = row else {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "unknown code",
                ));
            };
            let now = db::now();
            if used_at.is_some() || expires_at < now {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "code expired or already used",
                ));
            }
            if cid != client_id || f.redirect_uri.as_deref().is_some_and(|r| r != redirect_uri) {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "client_id or redirect_uri mismatch",
                ));
            }
            if ids::pkce_s256(&verifier) != challenge {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "PKCE verification failed",
                ));
            }
            sqlx::query("UPDATE oauth_codes SET used_at = ? WHERE code_hash = ?")
                .bind(now)
                .bind(ids::sha256_hex(code.as_bytes()))
                .execute(&st.db)
                .await?;
            Ok(no_store(
                Json(issue_tokens(&st, &client_id, owner_id).await?).into_response(),
            ))
        }
        "refresh_token" => {
            let Some(refresh) = f.refresh_token else {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "refresh_token required",
                ));
            };
            let hash = ids::sha256_hex(refresh.as_bytes());
            let row = sqlx::query_as::<_, (String, i64, i64, Option<i64>, Option<String>)>(
                "SELECT client_id, owner_id, expires_at, revoked_at, parent_hash FROM oauth_tokens WHERE token_hash = ? AND kind = 'refresh'",
            )
            .bind(&hash)
            .fetch_optional(&st.db)
            .await?;
            let Some((cid, owner_id, expires_at, revoked_at, parent)) = row else {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "unknown refresh token",
                ));
            };
            if revoked_at.is_some()
                || expires_at < db::now()
                || f.client_id.as_deref().is_some_and(|c| c != cid)
            {
                return Ok(oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_grant",
                    "refresh token invalid",
                ));
            }
            let now = db::now();
            sqlx::query(
                "UPDATE oauth_tokens SET revoked_at = ? WHERE token_hash = ? OR token_hash = ?",
            )
            .bind(now)
            .bind(&hash)
            .bind(parent.unwrap_or_default())
            .execute(&st.db)
            .await?;
            Ok(no_store(
                Json(issue_tokens(&st, &cid, owner_id).await?).into_response(),
            ))
        }
        _ => Ok(oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "use authorization_code or refresh_token",
        )),
    }
}

// ---- login / logout ----

#[derive(Deserialize)]
struct LoginQuery {
    next: Option<String>,
    error: Option<String>,
}

fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n.to_string(),
        _ => "/".to_string(),
    }
}

async fn login_get(Query(q): Query<LoginQuery>) -> Html<String> {
    let err = if q.error.is_some() {
        r#"<p class="error">Wrong owner slug or password.</p>"#
    } else {
        ""
    };
    let body = format!(
        r#"<h2>Sign in</h2>{err}
<form method="post" action="/login">
  <input type="hidden" name="next" value="{next}">
  <label>Owner slug<input name="slug" autocomplete="username" required autofocus></label>
  <label>Password or API key<input name="password" type="password" autocomplete="current-password" required></label>
  <button class="primary">Sign in</button>
</form>"#,
        next = escape(&safe_next(q.next.as_deref())),
    );
    Html(page_html("Sign in", &body))
}

#[derive(Deserialize)]
struct LoginForm {
    slug: String,
    password: String,
    next: Option<String>,
}

async fn login_post(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: header::HeaderMap,
    jar: CookieJar,
    Form(f): Form<LoginForm>,
) -> Response {
    let next = safe_next(f.next.as_deref());
    let ip = view::client_ip(&headers, Some(addr));
    if !st
        .limiter
        .check(&format!("login:{ip}"), 10, Duration::from_secs(900))
    {
        return AppError::RateLimited.into_response();
    }
    let Ok(owner) = crate::webauth::authenticate(&st, Some(&f.slug), &f.password).await else {
        return Redirect::to(&format!("/login?error=1&next={}", enc(&next))).into_response();
    };
    match auth::create_session(&st.db, owner.id).await {
        Ok(raw) => {
            let jar = jar.add(auth::cookie(
                &st.cfg,
                SESSION_COOKIE,
                raw,
                Some(auth::SESSION_TTL),
            ));
            (jar, Redirect::to(&next)).into_response()
        }
        Err(e) => AppError::Internal(e).into_response(),
    }
}

async fn logout(State(st): State<AppState>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(&st.cfg.cookie_name(SESSION_COOKIE)) {
        let _ = auth::delete_session(&st.db, c.value()).await;
    }
    let jar = jar.add(auth::remove_cookie(&st.cfg, SESSION_COOKIE));
    (jar, Redirect::to("/login")).into_response()
}

// ---- tiny server-rendered pages (owner-facing only) ----

fn page(title: &str, body: &str) -> Response {
    Html(page_html(title, body)).into_response()
}

fn page_html(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} · handout</title><style>
:root{{--bg:#f6f1e7;--card:#fffdf8;--fg:#1c1a16;--muted:#6f685c;--line:#e3dccd;--accent:#c2410c;--accent-fg:#fff}}
@media(prefers-color-scheme:dark){{:root{{--bg:#161411;--card:#1f1c17;--fg:#efe9dc;--muted:#9a9184;--line:#33302a;--accent:#fb923c;--accent-fg:#1c1a16}}}}
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--fg);font:16px/1.5 ui-sans-serif,system-ui,sans-serif}}
main{{width:min(26rem,92vw);background:var(--card);border:1px solid var(--line);border-radius:14px;padding:1.6rem 1.8rem;box-shadow:0 20px 50px -30px rgba(0,0,0,.4)}}
h1{{font-size:1.1rem;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);margin:0 0 1rem}}h2{{margin:.2rem 0 .8rem;font-size:1.5rem;letter-spacing:-.02em}}
label{{display:block;margin:.8rem 0 .3rem;font-size:.9rem;color:var(--muted)}}input{{width:100%;box-sizing:border-box;padding:.6rem .7rem;border:1px solid var(--line);border-radius:8px;background:transparent;color:inherit;font:inherit}}
button{{margin:.9rem .4rem 0 0;padding:.6rem 1.1rem;border-radius:8px;border:1px solid var(--line);background:transparent;color:inherit;font:inherit;cursor:pointer}}
button.primary{{background:var(--accent);color:var(--accent-fg);border-color:var(--accent)}}.muted{{color:var(--muted)}}.error{{color:#b91c1c}}
.tiny{{margin-top:1.2rem}}.tiny button{{font-size:.8rem;padding:.3rem .6rem;color:var(--muted)}}code{{color:var(--accent)}}
</style></head><body><main><h1>handout</h1>{body}</main></body></html>"#,
        title = escape(title),
        body = body
    )
}
